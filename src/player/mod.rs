mod engine;

use std::ffi::{CString, c_void};
use std::mem::ManuallyDrop;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use glutin::display::GlDisplay;
use libmpv2::Mpv;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};

use crate::plex::worker::Wake;
use engine::{Ask, Seen, Shared};

pub type ProcAddress = fn(&GlLookup, &str) -> *mut c_void;

pub struct GlLookup {
    pub display: glutin::display::Display,
}

pub fn proc_address(lookup: &GlLookup, name: &str) -> *mut c_void {
    CString::new(name).map(|name| lookup.display.get_proc_address(&name) as *mut c_void).unwrap_or(std::ptr::null_mut())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playback {
    pub idle: bool,
    pub paused: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub aspect: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Subtitle,
}

impl TrackKind {
    fn type_name(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Subtitle => "sub",
        }
    }

    fn property(self) -> &'static str {
        match self {
            Self::Audio => "aid",
            Self::Subtitle => "sid",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub id: i64,
    pub kind: TrackKind,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerEvent {
    Finished,
    Failed,
}

pub struct Player {
    render: ManuallyDrop<RenderContext<'static>>,
    asks: Sender<Ask>,
    events: Receiver<PlayerEvent>,
    shared: Shared,
}

fn configured() -> libmpv2::Result<Mpv> {
    Mpv::with_initializer(|init| {
        init.set_option("config", "no")?;
        init.set_option("vo", "libmpv")?;
        init.set_option("hwdec", "auto-safe")?;
        init.set_option("osc", "no")?;
        init.set_option("load-scripts", "no")?;
        init.set_option("ytdl", "no")?;
        init.set_option("input-default-bindings", "no")?;
        init.set_option("input-vo-keyboard", "no")?;
        init.set_option("idle", "yes")?;
        init.set_option("keep-open", "no")?;
        init.set_option("audio-client-name", "sth-plex-pip")?;
        init.set_option("demuxer-max-bytes", "256MiB")?;
        init.set_option("sub-auto", "no")?;
        crate::store::mpv_log().map_or(Ok(()), |path| init.set_option("log-file", path))
    })
}

impl Player {
    pub fn new(
        lookup: GlLookup,
        get_proc_address: ProcAddress,
        wayland_display: Option<*const c_void>,
        wake: Wake,
    ) -> libmpv2::Result<Self> {
        let mpv: &'static Mpv = Box::leak(Box::new(configured()?));
        let params = [
            Some(RenderParam::ApiType(RenderParamApiType::OpenGl)),
            Some(RenderParam::InitParams(OpenGLInitParams { get_proc_address, ctx: lookup })),
            wayland_display.map(RenderParam::WaylandDisplay),
        ];
        let render = {
            let mut render = mpv.create_render_context(params.into_iter().flatten())?;
            let on_frame = wake.clone();
            render.set_update_callback(move || on_frame());
            render
        };
        let (asks, asked) = channel::<Ask>();
        let (ended, events) = channel::<PlayerEvent>();
        let shared: Shared = Arc::new(Mutex::new(Seen::default()));
        engine::obey(mpv, asked, ended.clone(), shared.clone(), wake.clone());
        engine::watch(mpv, ended, shared.clone(), wake);
        Ok(Self { render: ManuallyDrop::new(render), asks, events, shared })
    }

    fn ask(&self, ask: Ask) {
        let _ = self.asks.send(ask);
    }

    fn seen(&self) -> Seen {
        self.shared.lock().map(|seen| seen.clone()).unwrap_or_default()
    }

    pub fn load(&self, url: &str, token: &str, start_seconds: f64) {
        self.ask(Ask::Load { url: url.to_string(), token: token.to_string(), start_seconds });
    }

    pub fn stop(&self) {
        self.ask(Ask::Stop);
    }

    pub fn set_paused(&self, paused: bool) {
        self.ask(Ask::SetPaused(paused));
    }

    pub fn toggle_pause(&self) {
        self.ask(Ask::TogglePause);
    }

    pub fn seek_by(&self, seconds: f64) {
        self.ask(Ask::SeekBy(seconds));
    }

    pub fn seek_to(&self, seconds: f64) {
        self.ask(Ask::SeekTo(seconds));
    }

    pub fn set_volume(&self, volume: f64) {
        self.ask(Ask::SetVolume(volume));
    }

    pub fn select(&self, kind: TrackKind, id: Option<i64>) {
        self.ask(Ask::Select(kind, id));
    }

    pub fn playback(&self) -> Playback {
        self.seen().playback
    }

    pub fn tracks(&self, kind: TrackKind) -> Vec<Track> {
        match kind {
            TrackKind::Audio => self.seen().audio,
            TrackKind::Subtitle => self.seen().subtitles,
        }
    }

    pub fn events(&self) -> Vec<PlayerEvent> {
        self.events.try_iter().collect()
    }

    pub fn draw(&self, framebuffer: i32, width: i32, height: i32) {
        let _ = self.render.update();
        let _ = self.render.render::<GlLookup>(framebuffer, width, height, false);
    }
}
