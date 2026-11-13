use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Ui, UiBuilder, Vec2};

use crate::command::Command;
use crate::placement::Corner;
use crate::player::{Playback, Track, TrackKind};
use crate::plex::model::{Item, ItemKind, Section};
use crate::plex::worker::Listing;

const BAR: Color32 = Color32::from_rgba_premultiplied(12, 12, 14, 215);
const PANEL: Color32 = Color32::from_rgb(22, 22, 26);
const ACCENT: Color32 = Color32::from_rgb(229, 160, 13);
const QUIET: Color32 = Color32::from_rgb(150, 150, 158);
const BAR_HEIGHT: f32 = 30.0;
const GRIP: f32 = 18.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SignIn,
    SignOut,
    Back,
    Open(Listing),
    Play { item: Box<Item>, following: Vec<Item> },
    Run(Command),
    SeekTo(f64),
    SetVolume(f64),
    Select(TrackKind, Option<i64>),
    Move,
    Resize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub listing: Listing,
    pub title: String,
    pub items: Option<Vec<Item>>,
    pub filter: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Browser {
    pub server: String,
    pub sections: Vec<Section>,
    pub on_deck: Vec<Item>,
    pub trail: Vec<Page>,
    pub search: String,
    pub notice: Option<String>,
}

pub struct Watching<'a> {
    pub title: &'a str,
    pub playback: Playback,
    pub audio: &'a [Track],
    pub subtitles: &'a [Track],
    pub corner: Corner,
}

pub fn style(context: &egui::Context) {
    context.set_visuals(egui::Visuals {
        panel_fill: PANEL,
        window_fill: PANEL,
        selection: egui::style::Selection { bg_fill: ACCENT.gamma_multiply(0.6), ..Default::default() },
        ..egui::Visuals::dark()
    });
    context.all_styles_mut(|style| style.interaction.selectable_labels = false);
}

pub fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.0) as u64;
    let (hours, minutes, rest) = (whole / 3600, (whole / 60) % 60, whole % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{rest:02}") } else { format!("{minutes}:{rest:02}") }
}

fn panel(ui: &mut Ui) {
    ui.painter().rect_filled(ui.max_rect(), CornerRadius::ZERO, PANEL);
}

fn drag_strip(ui: &mut Ui, rect: Rect) -> Option<Action> {
    let response = ui.interact(rect, ui.scope_id().with("move"), Sense::drag());
    response.drag_started().then_some(Action::Move)
}

fn padded(rect: Rect) -> Rect {
    rect.shrink2(Vec2::new(8.0, 3.0))
}

fn within<R>(ui: &mut Ui, rect: Rect, layout: Layout, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(UiBuilder::new().max_rect(rect).layout(layout), add).inner
}

pub fn sign_in(ui: &mut Ui, waiting: bool, notice: Option<&str>, link: Option<&str>) -> Vec<Action> {
    panel(ui);
    let strip = Rect::from_min_size(ui.max_rect().min, Vec2::new(ui.max_rect().width(), BAR_HEIGHT));
    let moved = drag_strip(ui, strip);
    let pressed = ui
        .vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            ui.label(RichText::new("Plex picture-in-picture").size(18.0).strong());
            ui.add_space(10.0);
            notice.into_iter().for_each(|text| {
                ui.colored_label(Color32::LIGHT_RED, text);
            });
            if waiting {
                ui.label("Finish signing in in the browser that just opened.");
                ui.add_space(6.0);
                ui.spinner();
                link.into_iter().for_each(|url| {
                    ui.add_space(10.0);
                    ui.label(RichText::new("No browser? Open this link on any device:").color(QUIET).small());
                    ui.add(egui::TextEdit::singleline(&mut url.to_string()).desired_width(420.0));
                });
                return None;
            }
            let button =
                egui::Button::new(RichText::new("Sign in with Plex").size(15.0)).fill(ACCENT.gamma_multiply(0.5));
            ui.add(button).clicked().then_some(Action::SignIn)
        })
        .inner;
    [moved, pressed].into_iter().flatten().collect()
}

pub fn connecting(ui: &mut Ui) -> Vec<Action> {
    panel(ui);
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.4);
        ui.spinner();
        ui.label("Finding your Plex server");
    });
    vec![]
}

fn marker(item: &Item) -> &'static str {
    if item.watched() {
        return "✔ ";
    }
    if item.view_offset.unwrap_or(0) > 0 {
        return "▶ ";
    }
    "    "
}

fn row_action(item: &Item, siblings: &[Item]) -> Action {
    if !item.playable() {
        return Action::Open(Listing::Children(Box::new(item.clone())));
    }
    let following = siblings
        .iter()
        .skip_while(|other| other.rating_key != item.rating_key)
        .skip(1)
        .filter(|other| other.kind() == ItemKind::Episode)
        .cloned()
        .collect();
    Action::Play { item: Box::new(item.clone()), following }
}

fn row(ui: &mut Ui, text: String, strong: bool) -> bool {
    let label = if strong { RichText::new(text).strong() } else { RichText::new(text) };
    ui.add_sized(
        Vec2::new(ui.available_width(), 22.0),
        egui::Button::new(label).frame(false).wrap_mode(egui::TextWrapMode::Truncate).right_text(""),
    )
    .clicked()
}

fn item_row(ui: &mut Ui, item: &Item, siblings: &[Item], heading: bool) -> Option<Action> {
    let name = if heading { item.heading() } else { item.label() };
    row(ui, format!("{}{name}", marker(item)), false).then(|| row_action(item, siblings))
}

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).color(QUIET).small());
}

fn home(ui: &mut Ui, browser: &Browser) -> Vec<Action> {
    let resumed: Vec<Action> = if browser.on_deck.is_empty() {
        vec![]
    } else {
        caption(ui, "CONTINUE WATCHING");
        browser.on_deck.iter().filter_map(|item| item_row(ui, item, &[], true)).collect()
    };
    caption(ui, "LIBRARIES");
    let opened = browser
        .sections
        .iter()
        .filter(|section| row(ui, format!("    {}", section.title), true))
        .map(|section| Action::Open(Listing::Section(section.clone())))
        .collect::<Vec<_>>();
    resumed.into_iter().chain(opened).collect()
}

fn matches(item: &Item, filter: &str) -> bool {
    filter.is_empty() || item.label().to_lowercase().contains(&filter.to_lowercase())
}

fn page(ui: &mut Ui, page: &Page) -> Vec<Action> {
    let Some(items) = &page.items else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| ui.spinner());
        return vec![];
    };
    let shown: Vec<&Item> = items.iter().filter(|item| matches(item, &page.filter)).collect();
    if shown.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("    Nothing here.").color(QUIET));
        return vec![];
    }
    let heading = matches!(page.listing, Listing::Search(_));
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, 22.0, shown.len(), |ui, range| {
            shown[range].iter().filter_map(|item| item_row(ui, item, items, heading)).collect::<Vec<_>>()
        })
        .inner
}

fn search_box(ui: &mut Ui, text: &mut String, hint: &str) -> bool {
    let response = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(150.0));
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

fn browser_bar(ui: &mut Ui, browser: &mut Browser) -> Vec<Action> {
    let deep = !browser.trail.is_empty();
    let back = deep.then(|| ui.button("⬅").clicked().then_some(Action::Back)).flatten();
    let title = browser.trail.last().map(|page| page.title.clone()).unwrap_or_else(|| browser.server.clone());
    ui.label(RichText::new(title).strong());
    let right = ui
        .with_layout(Layout::right_to_left(Align::Center), |ui| {
            let menu = ui
                .menu_button("☰", |ui| {
                    let out = ui.button("Sign out").clicked().then_some(Action::SignOut);
                    let quit = ui.button("Quit").clicked().then_some(Action::Run(Command::Quit));
                    out.or(quit)
                })
                .inner
                .flatten();
            let searched = match browser.trail.last_mut() {
                Some(page) if !matches!(page.listing, Listing::Search(_)) => {
                    search_box(ui, &mut page.filter, "Filter");
                    None
                }
                _ => search_box(ui, &mut browser.search, "Search Plex")
                    .then(|| Action::Open(Listing::Search(browser.search.clone())))
                    .filter(|_| !browser.search.trim().is_empty()),
            };
            [menu, searched]
        })
        .inner;
    [back].into_iter().chain(right).flatten().collect()
}

pub fn browser(ui: &mut Ui, browser: &mut Browser) -> Vec<Action> {
    panel(ui);
    let full = ui.max_rect();
    let bar = Rect::from_min_size(full.min, Vec2::new(full.width(), BAR_HEIGHT));
    let body = Rect::from_min_max(bar.left_bottom(), full.max).shrink2(Vec2::new(6.0, 2.0));
    let moved = drag_strip(ui, bar);
    let barred = within(ui, padded(bar), Layout::left_to_right(Align::Center), |ui| browser_bar(ui, browser));
    let listed = within(ui, body, Layout::top_down(Align::Min), |ui| {
        browser.notice.iter().for_each(|text| {
            ui.colored_label(Color32::LIGHT_RED, text);
        });
        match browser.trail.last() {
            Some(current) => page(ui, current),
            None => egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| home(ui, browser)).inner,
        }
    });
    moved.into_iter().chain(barred).chain(listed).collect()
}

pub fn video(ui: &mut Ui, texture: Option<egui::TextureId>, opacity: f32) {
    let rect = ui.max_rect();
    let tint = Color32::WHITE.gamma_multiply(opacity);
    ui.painter().rect_filled(rect, CornerRadius::ZERO, Color32::BLACK.gamma_multiply(opacity));
    texture.into_iter().for_each(|id| {
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(id, rect, uv, tint);
    });
}

fn track_menu(ui: &mut Ui, icon: &str, kind: TrackKind, tracks: &[Track], can_turn_off: bool) -> Option<Action> {
    ui.menu_button(icon, |ui| {
        let none_selected = !tracks.iter().any(|track| track.selected);
        let off = can_turn_off
            .then(|| ui.radio(none_selected, "Off").clicked().then_some(Action::Select(kind, None)))
            .flatten();
        let picked = tracks
            .iter()
            .filter_map(|track| {
                ui.radio(track.selected, &track.label).clicked().then_some(Action::Select(kind, Some(track.id)))
            })
            .next_back();
        off.or(picked)
    })
    .inner
    .flatten()
}

fn top_bar(ui: &mut Ui, watching: &Watching) -> Vec<Action> {
    ui.label(RichText::new(watching.title).strong().color(Color32::WHITE));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let done = ui.button("✖").on_hover_text("Put the controls away").clicked();
        let library = ui.button("☰").on_hover_text("Stop and go back to the library").clicked();
        [done.then_some(Action::Run(Command::Controls)), library.then_some(Action::Run(Command::Stop))]
    })
    .inner
    .into_iter()
    .flatten()
    .collect()
}

fn seek_bar(ui: &mut Ui, playback: &Playback) -> Option<Action> {
    let mut position = playback.position;
    ui.spacing_mut().slider_width = ui.available_width();
    let slider = egui::Slider::new(&mut position, 0.0..=playback.duration.max(1.0)).show_value(false);
    let response = ui.add(slider);
    (response.drag_stopped() || response.clicked() || response.changed()).then_some(Action::SeekTo(position))
}

fn transport(ui: &mut Ui, watching: &Watching) -> Vec<Action> {
    let playback = &watching.playback;
    let back = ui.button("⏪").clicked().then_some(Action::Run(Command::Seek { seconds: -10.0 }));
    let toggle = ui.button(if playback.paused { "▶" } else { "⏸" }).clicked().then_some(Action::Run(Command::Pause));
    let forward = ui.button("⏩").clicked().then_some(Action::Run(Command::Seek { seconds: 30.0 }));
    ui.label(
        RichText::new(format!("{} / {}", clock(playback.position), clock(playback.duration)))
            .color(Color32::WHITE)
            .monospace(),
    );
    let right = ui
        .with_layout(Layout::right_to_left(Align::Center), |ui| {
            let subtitles = track_menu(ui, "CC", TrackKind::Subtitle, watching.subtitles, true);
            let audio = track_menu(ui, "♫", TrackKind::Audio, watching.audio, false);
            let mut volume = playback.volume;
            ui.spacing_mut().slider_width = 70.0;
            let changed = ui.add(egui::Slider::new(&mut volume, 0.0..=100.0).show_value(false)).changed();
            ui.label("🔊");
            [subtitles, audio, changed.then_some(Action::SetVolume(volume))]
        })
        .inner;
    [back, toggle, forward].into_iter().chain(right).flatten().collect()
}

fn grip_rect(full: Rect, corner: Corner) -> Rect {
    let x = if corner.left() { full.right() - GRIP } else { full.left() };
    let y = if corner.top() { full.bottom() - GRIP } else { full.top() };
    Rect::from_min_size(egui::pos2(x, y), Vec2::splat(GRIP))
}

fn clear_of_grip(bar: Rect, corner: Corner) -> Rect {
    let (left, right) = if corner.left() { (0.0, GRIP) } else { (GRIP, 0.0) };
    Rect::from_min_max(bar.min + Vec2::new(left, 0.0), bar.max - Vec2::new(right, 0.0))
}

fn grip(ui: &mut Ui, corner: Corner) -> Option<Action> {
    let rect = grip_rect(ui.max_rect(), corner);
    let response = ui.interact(rect, ui.scope_id().with("resize"), Sense::drag());
    ui.painter().rect_filled(rect.shrink(4.0), CornerRadius::same(3), ACCENT);
    response.drag_started().then_some(Action::Resize)
}

pub fn controls(ui: &mut Ui, watching: &Watching) -> Vec<Action> {
    let full = ui.max_rect();
    let top = Rect::from_min_size(full.min, Vec2::new(full.width(), BAR_HEIGHT));
    let bottom = Rect::from_min_max(egui::pos2(full.left(), full.bottom() - 2.0 * BAR_HEIGHT), full.max);
    let (seek, buttons) = bottom.split_top_bottom_at_fraction(0.45);
    ui.painter().rect_filled(top, CornerRadius::ZERO, BAR);
    ui.painter().rect_filled(bottom, CornerRadius::ZERO, BAR);
    let moved = drag_strip(ui, top);
    let (title, buttons) = if watching.corner.top() {
        (top, clear_of_grip(buttons, watching.corner))
    } else {
        (clear_of_grip(top, watching.corner), buttons)
    };
    let barred = within(ui, padded(title), Layout::left_to_right(Align::Center), |ui| top_bar(ui, watching));
    let sought = within(ui, padded(seek), Layout::left_to_right(Align::Center), |ui| seek_bar(ui, &watching.playback));
    let pressed = within(ui, padded(buttons), Layout::left_to_right(Align::Center), |ui| transport(ui, watching));
    let resized = grip(ui, watching.corner);
    resized.or(moved).into_iter().chain(barred).chain(sought).chain(pressed).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_shows_hours_only_when_there_are_any() {
        assert_eq!(clock(75.0), "1:15");
        assert_eq!(clock(3675.0), "1:01:15");
    }

    fn episode(key: &str) -> Item {
        serde_json::from_str(&format!(
            r#"{{"ratingKey":"{key}","key":"/library/metadata/{key}","type":"episode","title":"E{key}"}}"#
        ))
        .unwrap()
    }

    #[test]
    fn playing_an_episode_queues_the_ones_after_it() {
        let season = [episode("1"), episode("2"), episode("3")];
        let Action::Play { following, .. } = row_action(&season[1], &season) else {
            panic!("an episode plays");
        };
        assert_eq!(following, vec![episode("3")]);
    }
}
