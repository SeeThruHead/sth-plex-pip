use std::ffi::{CString, c_void};

use glutin::display::GlDisplay;

use libmpv2::Mpv;
use libmpv2::events::Event;
use libmpv2::mpv_end_file_reason;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};

use crate::plex::worker::Wake;

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
    mpv: &'static Mpv,
    render: RenderContext<'static>,
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
        Ok(())
    })
}

impl Player {
    pub fn new(
        lookup: GlLookup,
        get_proc_address: ProcAddress,
        wayland_display: Option<*const c_void>,
        wake: Wake,
    ) -> libmpv2::Result<Self> {
        let mpv: &'static Mpv = {
            let mut mpv = configured()?;
            let on_event = wake.clone();
            mpv.set_wakeup_callback(move || on_event());
            Box::leak(Box::new(mpv))
        };
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
        Ok(Self { mpv, render })
    }

    pub fn load(&self, url: &str, token: &str, start_seconds: f64) -> libmpv2::Result<()> {
        self.mpv.set_property("http-header-fields", format!("X-Plex-Token: {token}"))?;
        self.mpv.set_property("pause", false)?;
        let start = format!("start={start_seconds:.3}");
        self.mpv.command("loadfile", &[url, "replace", "-1", &start])
    }

    pub fn stop(&self) {
        let _ = self.mpv.command("stop", &[]);
    }

    pub fn set_paused(&self, paused: bool) {
        let _ = self.mpv.set_property("pause", paused);
    }

    pub fn toggle_pause(&self) {
        self.set_paused(!self.flag("pause"));
    }

    pub fn seek_by(&self, seconds: f64) {
        let _ = self.mpv.command("seek", &[&format!("{seconds}"), "relative"]);
    }

    pub fn seek_to(&self, seconds: f64) {
        let _ = self.mpv.command("seek", &[&format!("{seconds}"), "absolute"]);
    }

    pub fn set_volume(&self, volume: f64) {
        let _ = self.mpv.set_property("volume", volume.clamp(0.0, 130.0));
    }

    pub fn select(&self, kind: TrackKind, id: Option<i64>) {
        let value = id.map(|id| id.to_string()).unwrap_or_else(|| "no".into());
        let _ = self.mpv.set_property(kind.property(), value);
    }

    fn flag(&self, name: &str) -> bool {
        self.mpv.get_property::<bool>(name).unwrap_or(false)
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.mpv.get_property::<f64>(name).ok()
    }

    fn text(&self, name: &str) -> Option<String> {
        self.mpv.get_property::<String>(name).ok().filter(|text| !text.is_empty())
    }

    pub fn playback(&self) -> Playback {
        Playback {
            idle: self.flag("idle-active"),
            paused: self.flag("pause"),
            position: self.number("time-pos").unwrap_or(0.0),
            duration: self.number("duration").unwrap_or(0.0),
            volume: self.number("volume").unwrap_or(100.0),
            aspect: self.number("video-params/aspect").filter(|aspect| *aspect > 0.1).map(|aspect| aspect as f32),
        }
    }

    fn track(&self, index: i64, kind: TrackKind) -> Option<Track> {
        let field = |name: &str| format!("track-list/{index}/{name}");
        if self.text(&field("type"))? != kind.type_name() {
            return None;
        }
        let id = self.mpv.get_property::<i64>(&field("id")).ok()?;
        let parts = [self.text(&field("lang")), self.text(&field("title")), self.text(&field("codec"))];
        let described = parts.into_iter().flatten().collect::<Vec<_>>().join(" · ");
        let label = if described.is_empty() { format!("Track {id}") } else { described };
        Some(Track { id, kind, label, selected: self.flag(&field("selected")) })
    }

    pub fn tracks(&self, kind: TrackKind) -> Vec<Track> {
        let count = self.mpv.get_property::<i64>("track-list/count").unwrap_or(0);
        (0..count).filter_map(|index| self.track(index, kind)).collect()
    }

    pub fn events(&self) -> Vec<PlayerEvent> {
        std::iter::from_fn(|| self.mpv.wait_event(0.0))
            .filter_map(|event| match event {
                Ok(Event::EndFile(reason)) if reason == mpv_end_file_reason::Eof => Some(PlayerEvent::Finished),
                Ok(Event::EndFile(reason)) if reason == mpv_end_file_reason::Error => Some(PlayerEvent::Failed),
                _ => None,
            })
            .collect()
    }

    pub fn draw(&self, framebuffer: i32, width: i32, height: i32) {
        let _ = self.render.update();
        let _ = self.render.render::<GlLookup>(framebuffer, width, height, false);
    }
}
