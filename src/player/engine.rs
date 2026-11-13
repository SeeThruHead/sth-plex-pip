use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use libmpv2::Mpv;
use libmpv2::events::Event;
use libmpv2::mpv_end_file_reason;

use super::{Playback, PlayerEvent, Track, TrackKind};
use crate::plex::worker::Wake;

const LOOK_EVERY_SECONDS: f64 = 0.25;

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    Load { url: String, token: String, start_seconds: f64 },
    Stop,
    SetPaused(bool),
    TogglePause,
    SeekBy(f64),
    SeekTo(f64),
    SetVolume(f64),
    Select(TrackKind, Option<i64>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub playback: Playback,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
}

impl Default for Seen {
    fn default() -> Self {
        let playback =
            Playback { idle: true, paused: false, position: 0.0, duration: 0.0, volume: 100.0, aspect: None };
        Self { playback, audio: vec![], subtitles: vec![] }
    }
}

pub type Shared = Arc<Mutex<Seen>>;

fn flag(mpv: &Mpv, name: &str) -> bool {
    mpv.get_property::<bool>(name).unwrap_or(false)
}

fn number(mpv: &Mpv, name: &str) -> Option<f64> {
    mpv.get_property::<f64>(name).ok()
}

fn text(mpv: &Mpv, name: &str) -> Option<String> {
    mpv.get_property::<String>(name).ok().filter(|text| !text.is_empty())
}

fn playback(mpv: &Mpv) -> Playback {
    Playback {
        idle: flag(mpv, "idle-active"),
        paused: flag(mpv, "pause"),
        position: number(mpv, "time-pos").unwrap_or(0.0),
        duration: number(mpv, "duration").unwrap_or(0.0),
        volume: number(mpv, "volume").unwrap_or(100.0),
        aspect: number(mpv, "video-params/aspect").filter(|aspect| *aspect > 0.1).map(|aspect| aspect as f32),
    }
}

fn track(mpv: &Mpv, index: i64, kind: TrackKind) -> Option<Track> {
    let field = |name: &str| format!("track-list/{index}/{name}");
    if text(mpv, &field("type"))? != kind.type_name() {
        return None;
    }
    let id = mpv.get_property::<i64>(&field("id")).ok()?;
    let parts = [text(mpv, &field("lang")), text(mpv, &field("title")), text(mpv, &field("codec"))];
    let described = parts.into_iter().flatten().collect::<Vec<_>>().join(" · ");
    let label = if described.is_empty() { format!("Track {id}") } else { described };
    Some(Track { id, kind, label, selected: flag(mpv, &field("selected")) })
}

fn tracks(mpv: &Mpv, kind: TrackKind) -> Vec<Track> {
    let count = mpv.get_property::<i64>("track-list/count").unwrap_or(0);
    (0..count).filter_map(|index| track(mpv, index, kind)).collect()
}

fn look(mpv: &Mpv, before: &Seen, tracks_too: bool) -> Seen {
    if !tracks_too {
        return Seen { playback: playback(mpv), ..before.clone() };
    }
    Seen { playback: playback(mpv), audio: tracks(mpv, TrackKind::Audio), subtitles: tracks(mpv, TrackKind::Subtitle) }
}

fn remembered(shared: &Shared) -> Seen {
    shared.lock().map(|seen| seen.clone()).unwrap_or_default()
}

fn publish(mpv: &Mpv, shared: &Shared, tracks_too: bool) -> bool {
    let fresh = look(mpv, &remembered(shared), tracks_too);
    shared.lock().map(|mut seen| std::mem::replace(&mut *seen, fresh.clone()) != fresh).unwrap_or(false)
}

fn load(mpv: &Mpv, url: &str, token: &str, start_seconds: f64) -> libmpv2::Result<()> {
    mpv.set_property("http-header-fields", format!("X-Plex-Token: {token}"))?;
    mpv.set_property("pause", false)?;
    mpv.command("loadfile", &[url, "replace", "-1", &format!("start={start_seconds:.3}")])
}

fn apply(mpv: &Mpv, ask: Ask) -> libmpv2::Result<()> {
    match ask {
        Ask::Load { url, token, start_seconds } => load(mpv, &url, &token, start_seconds),
        Ask::Stop => mpv.command("stop", &[]),
        Ask::SetPaused(paused) => mpv.set_property("pause", paused),
        Ask::TogglePause => mpv.set_property("pause", !flag(mpv, "pause")),
        Ask::SeekBy(seconds) => mpv.command("seek", &[&format!("{seconds}"), "relative"]),
        Ask::SeekTo(seconds) => mpv.command("seek", &[&format!("{seconds}"), "absolute"]),
        Ask::SetVolume(volume) => mpv.set_property("volume", volume.clamp(0.0, 130.0)),
        Ask::Select(kind, id) => {
            mpv.set_property(kind.property(), id.map(|id| id.to_string()).unwrap_or_else(|| "no".into()))
        }
    }
}

fn ended(event: libmpv2::Result<Event<'_>>) -> Option<PlayerEvent> {
    match event {
        Ok(Event::EndFile(reason)) if reason == mpv_end_file_reason::Eof => Some(PlayerEvent::Finished),
        Ok(Event::EndFile(reason)) if reason == mpv_end_file_reason::Error => Some(PlayerEvent::Failed),
        _ => None,
    }
}

pub fn obey(mpv: &'static Mpv, asks: Receiver<Ask>, events: Sender<PlayerEvent>, shared: Shared, wake: Wake) {
    thread::spawn(move || {
        asks.iter().for_each(|ask| {
            if apply(mpv, ask).is_err() {
                let _ = events.send(PlayerEvent::Failed);
            }
            publish(mpv, &shared, true);
            wake();
        })
    });
}

pub fn watch(mpv: &'static Mpv, events: Sender<PlayerEvent>, shared: Shared, wake: Wake) {
    thread::spawn(move || {
        std::iter::repeat_with(|| mpv.wait_event(LOOK_EVERY_SECONDS).map(ended)).for_each(|happened| {
            let told = happened.flatten().map(|event| events.send(event)).is_some();
            let changed = publish(mpv, &shared, happened.is_some());
            if told || changed {
                wake();
            }
        })
    });
}
