use egui::{Align, Color32, CornerRadius, Layout, Rect, RichText, Sense, Ui, UiBuilder, Vec2};

use crate::art::{Art, Shape};
use crate::command::Command;
use crate::placement::Corner;
use crate::player::{Playback, Track, TrackKind};
use crate::plex::model::{Item, ItemKind, Row, Section};
use crate::plex::worker::Listing;

const BAR: Color32 = Color32::from_rgba_premultiplied(12, 12, 14, 215);
const PANEL: Color32 = Color32::from_rgb(22, 22, 26);
const ACCENT: Color32 = Color32::from_rgb(229, 160, 13);
const QUIET: Color32 = Color32::from_rgb(150, 150, 158);
const CARD: Color32 = Color32::from_rgb(38, 38, 44);
const WIDE_CARD: f32 = 208.0;
const POSTER_CARD: f32 = 116.0;
const CARD_TEXT: f32 = 38.0;
const CARD_GAP: f32 = 10.0;
const EPISODE_ROW: f32 = 62.0;
const BAR_HEIGHT: f32 = 30.0;
const GRIP: f32 = 18.0;
const MENU_MARGIN: f32 = 40.0;

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
    pub rows: Vec<Row>,
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
    add(&mut ui.new_child(UiBuilder::new().max_rect(rect).layout(layout)))
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

fn caption(ui: &mut Ui, text: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(text).strong());
    ui.add_space(2.0);
}

fn picture(ui: &Ui, art: Option<&Art>, path: Option<&str>, shape: Shape, rect: Rect) {
    ui.painter().rect_filled(rect, CornerRadius::same(6), CARD);
    let found = art.zip(path).and_then(|(art, path)| art.picture(path, shape));
    found.into_iter().for_each(|found| {
        egui::Image::new((found.id, found.size)).corner_radius(CornerRadius::same(6)).paint_at(ui, rect);
    });
}

fn progress_strip(ui: &Ui, image: Rect, item: &Item) {
    item.progress().into_iter().for_each(|share| {
        let track = Rect::from_min_max(egui::pos2(image.left(), image.bottom() - 4.0), image.max);
        let done = Rect::from_min_size(track.min, Vec2::new(track.width() * share, track.height()));
        ui.painter().rect_filled(track, CornerRadius::ZERO, Color32::from_black_alpha(170));
        ui.painter().rect_filled(done, CornerRadius::ZERO, ACCENT);
    });
}

fn watched_badge(ui: &Ui, image: Rect, item: &Item) {
    if !item.watched() {
        return;
    }
    let spot = Rect::from_min_size(egui::pos2(image.right() - 20.0, image.top() + 4.0), Vec2::splat(16.0));
    ui.painter().rect_filled(spot, CornerRadius::same(8), Color32::from_black_alpha(190));
    ui.painter().text(spot.center(), egui::Align2::CENTER_CENTER, "✔", egui::FontId::proportional(10.0), ACCENT);
}

fn time_left(item: &Item) -> Option<String> {
    item.progress().and(item.minutes_left()).map(|minutes| format!("{minutes} min left"))
}

fn card_lines(item: &Item) -> (String, String) {
    let year = || item.year.map(|year| year.to_string()).unwrap_or_default();
    match item.kind() {
        ItemKind::Episode => (item.grandparent_title.clone().unwrap_or_else(|| item.title.clone()), item.label()),
        ItemKind::Season => (item.title.clone(), item.parent_title.clone().unwrap_or_default()),
        _ => (item.title.clone(), time_left(item).unwrap_or_else(year)),
    }
}

fn short(text: String, size: f32, color: Color32) -> egui::Label {
    egui::Label::new(RichText::new(text).size(size).color(color)).truncate()
}

fn card(ui: &mut Ui, art: Option<&Art>, item: &Item, shape: Shape, width: f32) -> bool {
    let image_size = Vec2::new(width, (width / shape.aspect()).round());
    let (rect, response) = ui.allocate_exact_size(image_size + Vec2::new(0.0, CARD_TEXT), Sense::click());
    let image = Rect::from_min_size(rect.min, image_size);
    let path = if shape == Shape::Still { item.still() } else { item.poster() };
    picture(ui, art, path, shape, image);
    progress_strip(ui, image, item);
    watched_badge(ui, image, item);
    if response.hovered() {
        ui.painter().rect_stroke(image, CornerRadius::same(6), (2.0, ACCENT), egui::StrokeKind::Inside);
    }
    let (first, second) = card_lines(item);
    let text = Rect::from_min_max(egui::pos2(rect.left(), image.bottom() + 3.0), rect.max);
    within(ui, text, Layout::top_down(Align::Min), |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add(short(first, 12.5, Color32::WHITE));
        ui.add(short(second, 11.0, QUIET));
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn shelf(ui: &mut Ui, art: Option<&Art>, row: &Row) -> Option<Action> {
    caption(ui, &row.title);
    let (shape, width) = if row.resumes() { (Shape::Still, WIDE_CARD) } else { (Shape::Poster, POSTER_CARD) };
    egui::ScrollArea::horizontal()
        .id_salt(&row.id)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = CARD_GAP;
                let picked =
                    row.items.iter().filter(|item| card(ui, art, item, shape, width)).map(|item| row_action(item, &[]));
                picked.last()
            })
            .inner
        })
        .inner
}

fn libraries(ui: &mut Ui, sections: &[Section]) -> Option<Action> {
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        sections
            .iter()
            .filter(|section| ui.add(egui::Button::new(&section.title).corner_radius(CornerRadius::same(12))).clicked())
            .map(|section| Action::Open(Listing::Section(section.clone())))
            .last()
    })
    .inner
}

fn home(ui: &mut Ui, art: Option<&Art>, browser: &Browser) -> Vec<Action> {
    let opened = libraries(ui, &browser.sections);
    if browser.rows.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.spinner());
    }
    let picked: Vec<Action> = browser.rows.iter().filter_map(|row| shelf(ui, art, row)).collect();
    ui.add_space(10.0);
    opened.into_iter().chain(picked).collect()
}

fn matches(item: &Item, filter: &str) -> bool {
    filter.is_empty() || item.label().to_lowercase().contains(&filter.to_lowercase())
}

fn episode_row(ui: &mut Ui, art: Option<&Art>, item: &Item) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), EPISODE_ROW), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), CARD);
    }
    let image = Rect::from_min_size(rect.min + Vec2::splat(3.0), Vec2::new(100.0, EPISODE_ROW - 6.0));
    picture(ui, art, item.still(), Shape::Still, image);
    progress_strip(ui, image, item);
    watched_badge(ui, image, item);
    let text = Rect::from_min_max(egui::pos2(image.right() + 10.0, rect.top() + 12.0), rect.max);
    let minutes = item.duration.map(|duration| format!("{} min", duration / 60_000)).unwrap_or_default();
    within(ui, text, Layout::top_down(Align::Min), |ui| {
        ui.add(short(item.label(), 13.5, Color32::WHITE));
        ui.add(short(time_left(item).unwrap_or(minutes), 11.5, QUIET));
    });
    response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn episode_list(ui: &mut Ui, art: Option<&Art>, shown: &[&Item], all: &[Item]) -> Vec<Action> {
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, EPISODE_ROW, shown.len(), |ui, range| {
            shown[range].iter().filter(|item| episode_row(ui, art, item)).map(|item| row_action(item, all)).collect()
        })
        .inner
}

fn poster_grid(ui: &mut Ui, art: Option<&Art>, shown: &[&Item], all: &[Item]) -> Vec<Action> {
    let columns = ((ui.available_width() + CARD_GAP) / (POSTER_CARD + CARD_GAP)).floor().max(1.0) as usize;
    let lines: Vec<&[&Item]> = shown.chunks(columns).collect();
    let height = (POSTER_CARD / Shape::Poster.aspect()).round() + CARD_TEXT + 6.0;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show_rows(ui, height, lines.len(), |ui, range| {
            lines[range]
                .iter()
                .flat_map(|line| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = CARD_GAP;
                        ui.set_height(height);
                        line.iter()
                            .filter(|item| card(ui, art, item, Shape::Poster, POSTER_CARD))
                            .map(|item| row_action(item, all))
                            .collect::<Vec<_>>()
                    })
                    .inner
                })
                .collect()
        })
        .inner
}

fn page(ui: &mut Ui, art: Option<&Art>, page: &Page) -> Vec<Action> {
    let Some(items) = &page.items else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.spinner());
        return vec![];
    };
    let shown: Vec<&Item> = items.iter().filter(|item| matches(item, &page.filter)).collect();
    if shown.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("Nothing here.").color(QUIET));
        return vec![];
    }
    let episodes = shown.iter().all(|item| item.kind() == ItemKind::Episode);
    let season = matches!(page.listing, Listing::Children(_));
    if episodes && season { episode_list(ui, art, &shown, items) } else { poster_grid(ui, art, &shown, items) }
}

fn search_box(ui: &mut Ui, text: &mut String, hint: &str) -> bool {
    let response = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(170.0));
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

fn browser_bar(ui: &mut Ui, browser: &mut Browser) -> Vec<Action> {
    let deep = !browser.trail.is_empty();
    let back = deep.then(|| ui.button("⬅").clicked().then_some(Action::Back)).flatten();
    let title = browser.trail.last().map(|page| page.title.clone()).unwrap_or_else(|| browser.server.clone());
    ui.add(egui::Label::new(RichText::new(title).strong()).truncate());
    let right = ui
        .with_layout(Layout::right_to_left(Align::Center), |ui| {
            let close = ui.button("✖").on_hover_text("Close the player").clicked();
            let menu =
                ui.menu_button("☰", |ui| ui.button("Sign out").clicked().then_some(Action::SignOut)).inner.flatten();
            let searched = match browser.trail.last_mut() {
                Some(page) if !matches!(page.listing, Listing::Search(_)) => {
                    search_box(ui, &mut page.filter, "Filter");
                    None
                }
                _ => search_box(ui, &mut browser.search, "Search Plex")
                    .then(|| Action::Open(Listing::Search(browser.search.clone())))
                    .filter(|_| !browser.search.trim().is_empty()),
            };
            [close.then_some(Action::Run(Command::Quit)), menu, searched]
        })
        .inner;
    [back].into_iter().chain(right).flatten().collect()
}

pub fn browser(ui: &mut Ui, art: Option<&Art>, browser: &mut Browser, corner: Corner) -> Vec<Action> {
    panel(ui);
    let full = ui.max_rect();
    let bar = Rect::from_min_size(full.min, Vec2::new(full.width(), BAR_HEIGHT));
    let body = Rect::from_min_max(bar.left_bottom(), full.max).shrink2(Vec2::new(10.0, 2.0));
    let moved = drag_strip(ui, bar);
    let clear = if corner.top() { bar } else { clear_of_grip(bar, corner) };
    let barred = within(ui, padded(clear), Layout::left_to_right(Align::Center), |ui| browser_bar(ui, browser));
    let listed = within(ui, body, Layout::top_down(Align::Min), |ui| {
        browser.notice.iter().for_each(|text| {
            ui.colored_label(Color32::LIGHT_RED, text);
        });
        match browser.trail.last() {
            Some(current) => page(ui, art, current),
            None => egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| home(ui, art, browser)).inner,
        }
    });
    let resized = grip(ui, corner);
    resized.or(moved).into_iter().chain(barred).chain(listed).collect()
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
    let room = ui.ctx().content_rect().height() - MENU_MARGIN;
    ui.menu_button(icon, |ui| {
        egui::ScrollArea::vertical().max_height(room).show(ui, |ui| track_choices(ui, kind, tracks, can_turn_off)).inner
    })
    .inner
    .flatten()
}

fn track_choices(ui: &mut Ui, kind: TrackKind, tracks: &[Track], can_turn_off: bool) -> Option<Action> {
    let none_selected = !tracks.iter().any(|track| track.selected);
    let off =
        can_turn_off.then(|| ui.radio(none_selected, "Off").clicked().then_some(Action::Select(kind, None))).flatten();
    let picked = tracks
        .iter()
        .filter_map(|track| {
            ui.radio(track.selected, &track.label).clicked().then_some(Action::Select(kind, Some(track.id)))
        })
        .next_back();
    off.or(picked)
}

fn top_bar(ui: &mut Ui, watching: &Watching) -> Vec<Action> {
    ui.label(RichText::new(watching.title).strong().color(Color32::WHITE));
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let close = ui.button("✖").on_hover_text("Stop and close the player").clicked();
        let library = ui.button("☰").on_hover_text("Stop and pick something else").clicked();
        [close.then_some(Action::Run(Command::Quit)), library.then_some(Action::Run(Command::Stop))]
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

fn picture_area(ui: &mut Ui, rect: Rect, playback: &Playback) -> Vec<Action> {
    let response = ui.interact(rect, ui.scope_id().with("picture"), Sense::click_and_drag());
    let wheel = ui.input(|input| input.smooth_scroll_delta.y) as f64;
    let turned = (response.hovered() && wheel != 0.0).then_some(Action::SetVolume(playback.volume + wheel * 0.15));
    let icon = if playback.paused { "▶" } else { "⏸" };
    let spot = Rect::from_center_size(rect.center(), Vec2::splat(54.0));
    ui.painter().rect_filled(spot, CornerRadius::same(27), BAR);
    ui.painter().text(
        spot.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(26.0),
        Color32::WHITE,
    );
    let toggled = response.clicked().then_some(Action::Run(Command::Pause));
    let moved = response.drag_started().then_some(Action::Move);
    [turned, toggled, moved].into_iter().flatten().collect()
}

pub fn controls(ui: &mut Ui, watching: &Watching) -> Vec<Action> {
    let full = ui.max_rect();
    let top = Rect::from_min_size(full.min, Vec2::new(full.width(), BAR_HEIGHT));
    let bottom = Rect::from_min_max(egui::pos2(full.left(), full.bottom() - 2.0 * BAR_HEIGHT), full.max);
    let (seek, buttons) = bottom.split_top_bottom_at_fraction(0.45);
    ui.painter().rect_filled(top, CornerRadius::ZERO, BAR);
    ui.painter().rect_filled(bottom, CornerRadius::ZERO, BAR);
    let moved = drag_strip(ui, top);
    let middle = Rect::from_min_max(top.left_bottom(), bottom.right_top());
    let pictured = picture_area(ui, middle, &watching.playback);
    let (title, buttons) = if watching.corner.top() {
        (top, clear_of_grip(buttons, watching.corner))
    } else {
        (clear_of_grip(top, watching.corner), buttons)
    };
    let barred = within(ui, padded(title), Layout::left_to_right(Align::Center), |ui| top_bar(ui, watching));
    let sought = within(ui, padded(seek), Layout::left_to_right(Align::Center), |ui| seek_bar(ui, &watching.playback));
    let pressed = within(ui, padded(buttons), Layout::left_to_right(Align::Center), |ui| transport(ui, watching));
    let resized = grip(ui, watching.corner);
    resized.or(moved).into_iter().chain(pictured).chain(barred).chain(sought).chain(pressed).collect()
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
