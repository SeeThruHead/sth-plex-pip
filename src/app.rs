use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui_glow::Painter;
use egui_glow::glow::{self, HasContext};

use crate::command::Command;
use crate::placement::{self, Corner, Gesture, Size};
use crate::player::{Playback, Player, PlayerEvent, TrackKind};
use crate::plex::client::{Identity, Library, PlayState, Progress};
use crate::plex::model::Item;
use crate::plex::worker::{Listing, PlexWorker, Reply, Request, Wake};
use crate::presence::{Look, Presence};
use crate::store::{self, Auth, Settings};
use crate::ui::{self, Action, Browser, Page, Watching};

const REPORT_EVERY: Duration = Duration::from_secs(10);
const CONTROLS_REFRESH: Duration = Duration::from_millis(250);
const WIDESCREEN: f32 = 16.0 / 9.0;
const LIBRARY_SIZE: Size = Size { width: 560.0, height: 420.0 };

pub struct FrameInput {
    pub raw: egui::RawInput,
    pub pixels: [u32; 2],
    pub hovered: bool,
    pub modifier_held: bool,
    pub window_fades: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wants {
    pub look: Look,
    pub size: Size,
    pub aspect: f32,
    pub corner: Corner,
    pub keyboard: bool,
    pub gesture: Option<Gesture>,
    pub quit: bool,
    pub repaint_after: Option<Duration>,
}

enum Session {
    SignedOut { waiting: bool, notice: Option<String>, link: Option<String> },
    Connecting,
    Ready(Library),
}

struct NowPlaying {
    item: Item,
    following: Vec<Item>,
    reported: Instant,
    state: PlayState,
}

pub struct App {
    context: egui::Context,
    painter: Painter,
    gl: Arc<glow::Context>,
    target: crate::video::VideoTarget,
    player: Player,
    plex: PlexWorker,
    commands: Receiver<Command>,
    auth: Auth,
    settings: Settings,
    session: Session,
    browser: Browser,
    now: Option<NowPlaying>,
    wanted: Option<String>,
    controls_wanted: bool,
    hidden: bool,
    quit: bool,
}

fn title_of(listing: &Listing) -> String {
    match listing {
        Listing::OnDeck => "Continue watching".into(),
        Listing::Section(section) => section.title.clone(),
        Listing::Children(item) => item.heading(),
        Listing::Search(words) => format!("Search: {words}"),
    }
}

fn play_state(playback: &Playback) -> PlayState {
    if playback.paused { PlayState::Paused } else { PlayState::Playing }
}

impl App {
    pub fn new(
        gl: Arc<glow::Context>,
        player: Player,
        commands: Receiver<Command>,
        wake: Wake,
    ) -> Result<Self, String> {
        let painter = Painter::new(gl.clone(), "", None, false).map_err(|error| error.to_string())?;
        let context = egui::Context::default();
        ui::style(&context);
        let auth = store::auth();
        let settings = store::settings();
        let plex = PlexWorker::start(Identity { client_id: auth.client_id.clone() }, wake);
        player.set_volume(settings.volume);
        let session = match &auth.token {
            Some(token) => {
                plex.ask(Request::Connect { token: token.clone() });
                Session::Connecting
            }
            None => Session::SignedOut { waiting: false, notice: None, link: None },
        };
        Ok(Self {
            target: crate::video::VideoTarget::new(gl.clone()),
            context,
            painter,
            gl,
            player,
            plex,
            commands,
            auth,
            settings,
            session,
            browser: Browser::default(),
            now: None,
            wanted: None,
            controls_wanted: false,
            hidden: false,
            quit: false,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn context(&self) -> egui::Context {
        self.context.clone()
    }

    pub fn placed(&mut self, corner: Corner, width: f32) {
        self.settings = Settings {
            corner,
            width: if self.now.is_some() { width } else { self.settings.width },
            ..self.settings.clone()
        };
        let _ = store::save_settings(&self.settings);
    }

    fn progress(&self, playing: &NowPlaying, state: PlayState) -> Progress {
        let playback = self.player.playback();
        Progress {
            rating_key: playing.item.rating_key.clone(),
            key: playing.item.key.clone(),
            state,
            position_ms: (playback.position * 1000.0) as u64,
            duration_ms: (playback.duration * 1000.0) as u64,
        }
    }

    fn tell_plex(&self, update: Request) {
        if self.settings.update_plex {
            self.plex.ask(update);
        }
    }

    fn report(&mut self, state: PlayState) {
        let Some(playing) = self.now.as_ref() else { return };
        let progress = self.progress(playing, state);
        if progress.duration_ms == 0 {
            return;
        }
        self.tell_plex(Request::Report(progress));
        self.now = self.now.take().map(|playing| NowPlaying { reported: Instant::now(), state, ..playing });
    }

    fn report_when_due(&mut self) {
        let Some(playing) = self.now.as_ref() else { return };
        let state = play_state(&self.player.playback());
        if state == playing.state && playing.reported.elapsed() < REPORT_EVERY {
            return;
        }
        self.report(state);
    }

    fn play(&mut self, item: Item, following: Vec<Item>) {
        let Session::Ready(library) = &self.session else { return };
        let Some(part) = item.part_key() else {
            self.browser.notice = Some(format!("Plex has no file for {}.", item.label()));
            return;
        };
        let url = library.stream_url(part);
        if let Err(error) = self.player.load(&url, &library.server().token, item.resume_seconds()) {
            self.browser.notice = Some(format!("Could not start playback: {error}"));
            return;
        }
        if following.is_empty() {
            self.plex.ask(Request::Following { episode: Box::new(item.clone()) });
        }
        self.browser.notice = None;
        self.controls_wanted = false;
        self.now = Some(NowPlaying { item, following, reported: Instant::now(), state: PlayState::Playing });
    }

    fn refresh_home(&self) {
        self.plex.ask(Request::List(Listing::OnDeck));
    }

    fn stop(&mut self) {
        self.report(PlayState::Stopped);
        self.player.stop();
        self.now = None;
        self.controls_wanted = false;
        self.refresh_home();
    }

    fn finished(&mut self) {
        let Some(playing) = self.now.take() else { return };
        self.tell_plex(Request::MarkWatched { rating_key: playing.item.rating_key.clone() });
        match playing.following.split_first() {
            Some((next, rest)) => self.play(Item { view_offset: None, ..next.clone() }, rest.to_vec()),
            None => self.refresh_home(),
        }
    }

    fn failed(&mut self) {
        let name = self.now.take().map(|playing| playing.item.label()).unwrap_or_default();
        self.browser.notice = Some(format!("{name} could not be played."));
    }

    fn save(&mut self, settings: Settings) {
        self.settings = settings;
        let _ = store::save_settings(&self.settings);
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Controls => self.controls_wanted = !self.controls_wanted,
            Command::Pause => self.player.toggle_pause(),
            Command::Seek { seconds } => self.player.seek_by(seconds),
            Command::Volume { change } => self.set_volume(self.player.playback().volume + change),
            Command::Hide => self.toggle_hidden(),
            Command::Opacity { change } => self
                .save(Settings { opacity: (self.settings.opacity + change).clamp(0.2, 1.0), ..self.settings.clone() }),
            Command::Corner { corner } => self.save(Settings { corner, ..self.settings.clone() }),
            Command::Play { words } => self.play_matching(words.join(" ")),
            Command::Stop => self.stop(),
            Command::Quit => {
                self.report(PlayState::Stopped);
                self.quit = true;
            }
        }
    }

    fn play_matching(&mut self, words: String) {
        if words.trim().is_empty() {
            self.play_first(self.browser.on_deck.clone());
            return;
        }
        self.wanted = Some(words.clone());
        self.plex.ask(Request::List(Listing::Search(words)));
    }

    fn play_first(&mut self, items: Vec<Item>) {
        match items.into_iter().find(Item::playable) {
            Some(item) => self.play(item, vec![]),
            None => self.browser.notice = Some("Nothing playable matched.".into()),
        }
    }

    fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 100.0);
        self.player.set_volume(volume);
        self.save(Settings { volume, ..self.settings.clone() });
    }

    fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
        if self.hidden {
            self.player.set_paused(true);
        }
    }

    fn sign_out(&mut self, notice: Option<String>) {
        self.stop();
        self.auth = Auth { token: None, ..self.auth.clone() };
        let _ = store::save_auth(&self.auth);
        self.browser = Browser::default();
        self.session = Session::SignedOut { waiting: false, notice, link: None };
    }

    fn open(&mut self, listing: Listing) {
        let page = Page { title: title_of(&listing), listing: listing.clone(), items: None, filter: String::new() };
        let replaces_search = matches!(
            (self.browser.trail.last().map(|page| &page.listing), &listing),
            (Some(Listing::Search(_)), Listing::Search(_))
        );
        if replaces_search {
            self.browser.trail.pop();
        }
        self.browser.trail.push(page);
        self.plex.ask(Request::List(listing));
    }

    fn act(&mut self, action: Action) -> Option<Gesture> {
        match action {
            Action::SignIn => {
                self.plex.ask(Request::SignIn);
                self.session = Session::SignedOut { waiting: true, notice: None, link: None };
            }
            Action::SignOut => self.sign_out(None),
            Action::Back => {
                self.browser.trail.pop();
            }
            Action::Open(listing) => self.open(listing),
            Action::Play { item, following } => self.play(*item, following),
            Action::Run(command) => self.command(command),
            Action::SeekTo(seconds) => self.player.seek_to(seconds),
            Action::SetVolume(volume) => self.set_volume(volume),
            Action::Select(kind, id) => self.player.select(kind, id),
            Action::Move => return Some(Gesture::Move),
            Action::Resize => return Some(Gesture::Resize),
        }
        None
    }

    fn listed(&mut self, listing: Listing, items: Vec<Item>) {
        if listing == Listing::OnDeck {
            self.browser.on_deck = items;
            return;
        }
        if self.wanted.as_ref().is_some_and(|words| listing == Listing::Search(words.clone())) {
            self.wanted = None;
            self.play_first(items);
            return;
        }
        self.browser.trail = std::mem::take(&mut self.browser.trail)
            .into_iter()
            .map(|page| if page.listing == listing { Page { items: Some(items.clone()), ..page } } else { page })
            .collect();
    }

    fn queued(&mut self, rating_key: String, items: Vec<Item>) {
        self.now = self.now.take().map(|playing| {
            if playing.item.rating_key == rating_key { NowPlaying { following: items, ..playing } } else { playing }
        });
    }

    fn hear(&mut self, reply: Reply) {
        match reply {
            Reply::SignInOpened { url } => {
                self.session = Session::SignedOut { waiting: true, notice: None, link: Some(url) };
            }
            Reply::SignedIn { token } => {
                self.auth = Auth { token: Some(token.clone()), ..self.auth.clone() };
                let _ = store::save_auth(&self.auth);
                self.session = Session::Connecting;
                self.plex.ask(Request::Connect { token });
            }
            Reply::Connected { library } => {
                self.browser = Browser { server: library.server().name.clone(), ..Browser::default() };
                self.session = Session::Ready(library);
                self.plex.ask(Request::Sections);
                self.refresh_home();
            }
            Reply::Sections(sections) => self.browser.sections = sections,
            Reply::Listed { listing, items } => self.listed(listing, items),
            Reply::Following { rating_key, items } => self.queued(rating_key, items),
            Reply::SignedOut => self.sign_out(Some("Plex asked for a fresh sign-in.".into())),
            Reply::Failed(message) => self.complain(message),
        }
    }

    fn complain(&mut self, message: String) {
        match self.session {
            Session::Ready(_) => self.browser.notice = Some(message),
            _ => self.session = Session::SignedOut { waiting: false, notice: Some(message), link: None },
        }
    }

    fn tick(&mut self) {
        let commands: Vec<Command> = self.commands.try_iter().collect();
        commands.into_iter().for_each(|command| self.command(command));
        self.plex.replies().into_iter().for_each(|reply| self.hear(reply));
        self.player.events().into_iter().for_each(|event| match event {
            PlayerEvent::Finished => self.finished(),
            PlayerEvent::Failed => self.failed(),
        });
        self.report_when_due();
    }

    fn look(&self, hovered: bool, modifier_held: bool) -> Look {
        Presence {
            playing: self.now.is_some(),
            controls_wanted: self.controls_wanted,
            modifier_held: modifier_held && hovered,
            hovered,
            hidden: self.hidden,
            opacity: self.settings.opacity,
            hover_opacity: self.settings.hover_opacity,
        }
        .look()
    }

    fn draw_video(&mut self, pixels: [u32; 2]) {
        self.target.fit(&mut self.painter, [pixels[0] as i32, pixels[1] as i32]);
        if let Some((framebuffer, size)) = self.target.framebuffer() {
            self.player.draw(framebuffer, size[0], size[1]);
        }
    }

    fn screen(&mut self, ui: &mut egui::Ui, look: Look, playback: &Playback) -> Vec<Action> {
        if !look.visible {
            return vec![];
        }
        if let Some(playing) = &self.now {
            ui::video(ui, self.target.texture(), look.opacity);
            if !look.controls {
                return vec![];
            }
            let title = playing.item.heading();
            let audio = self.player.tracks(TrackKind::Audio);
            let subtitles = self.player.tracks(TrackKind::Subtitle);
            let watching = Watching {
                title: &title,
                playback: *playback,
                audio: &audio,
                subtitles: &subtitles,
                corner: self.settings.corner,
            };
            return ui::controls(ui, &watching);
        }
        match &self.session {
            Session::SignedOut { waiting, notice, link } => {
                ui::sign_in(ui, *waiting, notice.as_deref(), link.as_deref())
            }
            Session::Connecting => ui::connecting(ui),
            Session::Ready(_) => ui::browser(ui, &mut self.browser),
        }
    }

    fn paint(&mut self, pixels: [u32; 2], output: egui::FullOutput) {
        let primitives = self.context.tessellate(output.shapes, output.pixels_per_point);
        let mut textures = output.textures_delta;
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.viewport(0, 0, pixels[0] as i32, pixels[1] as i32);
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
        self.painter.paint_and_update_textures(pixels, output.pixels_per_point, &primitives, &mut textures);
    }

    pub fn frame(&mut self, input: FrameInput) -> Wants {
        self.tick();
        let playback = self.player.playback();
        let (hovered, modifier_held) = (input.hovered, input.modifier_held);
        let look = self.look(hovered, modifier_held);
        let playing = self.now.is_some();
        if playing && look.visible {
            self.draw_video(input.pixels);
        }
        let painted = Look { opacity: if input.window_fades { 1.0 } else { look.opacity }, ..look };
        let context = self.context.clone();
        let mut actions: Vec<Action> = vec![];
        let output = context.run_ui(input.raw, |ui| {
            actions = self.screen(ui, painted, &playback);
        });
        let gesture = actions.into_iter().filter_map(|action| self.act(action)).next_back();
        let repaint = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|viewport| viewport.repaint_delay)
            .filter(|delay| *delay < Duration::from_secs(5));
        self.paint(input.pixels, output);
        let aspect = playback.aspect.unwrap_or(WIDESCREEN);
        let still_playing = self.now.is_some();
        Wants {
            look: self.look(hovered, modifier_held),
            size: if still_playing { placement::sized(self.settings.width, aspect) } else { LIBRARY_SIZE },
            aspect,
            corner: self.settings.corner,
            keyboard: !still_playing && !self.hidden,
            gesture,
            quit: self.quit,
            repaint_after: if still_playing && look.controls { Some(CONTROLS_REFRESH) } else { repaint },
        }
    }
}
