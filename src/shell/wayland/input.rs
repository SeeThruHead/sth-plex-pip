use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

const LEFT: u32 = 0x110;
const RIGHT: u32 = 0x111;
const MIDDLE: u32 = 0x112;

pub fn modifiers(held: smithay_client_toolkit::seat::keyboard::Modifiers) -> Modifiers {
    Modifiers {
        alt: held.alt,
        ctrl: held.ctrl,
        shift: held.shift,
        mac_cmd: false,
        command: held.ctrl,
    }
}

fn button(code: u32) -> Option<PointerButton> {
    match code {
        LEFT => Some(PointerButton::Primary),
        RIGHT => Some(PointerButton::Secondary),
        MIDDLE => Some(PointerButton::Middle),
        _ => None,
    }
}

pub fn position(event: &PointerEvent) -> Pos2 {
    Pos2::new(event.position.0 as f32, event.position.1 as f32)
}

pub fn is_primary_release(event: &PointerEvent) -> bool {
    matches!(event.kind, PointerEventKind::Release { button: LEFT, .. })
}

pub fn pointer(event: &PointerEvent, held: Modifiers) -> Option<Event> {
    let pos = position(event);
    match event.kind {
        PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => Some(Event::PointerMoved(pos)),
        PointerEventKind::Leave { .. } => Some(Event::PointerGone),
        PointerEventKind::Press { button: code, .. } => button(code)
            .map(|button| Event::PointerButton { pos, button, pressed: true, modifiers: held }),
        PointerEventKind::Release { button: code, .. } => button(code)
            .map(|button| Event::PointerButton { pos, button, pressed: false, modifiers: held }),
        PointerEventKind::Axis { horizontal, vertical, .. } => Some(Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(-horizontal.absolute as f32, -vertical.absolute as f32),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: held,
        }),
    }
}

fn named(keysym: Keysym) -> Option<Key> {
    match keysym {
        Keysym::BackSpace => Some(Key::Backspace),
        Keysym::Delete => Some(Key::Delete),
        Keysym::Return | Keysym::KP_Enter => Some(Key::Enter),
        Keysym::Escape => Some(Key::Escape),
        Keysym::Tab => Some(Key::Tab),
        Keysym::Left => Some(Key::ArrowLeft),
        Keysym::Right => Some(Key::ArrowRight),
        Keysym::Up => Some(Key::ArrowUp),
        Keysym::Down => Some(Key::ArrowDown),
        Keysym::Home => Some(Key::Home),
        Keysym::End => Some(Key::End),
        Keysym::Page_Up => Some(Key::PageUp),
        Keysym::Page_Down => Some(Key::PageDown),
        Keysym::a | Keysym::A => Some(Key::A),
        _ => None,
    }
}

fn typed(event: &KeyEvent, held: Modifiers) -> Option<Event> {
    event
        .utf8
        .clone()
        .filter(|text| !held.ctrl && !held.alt && text.chars().all(|letter| !letter.is_control()))
        .map(Event::Text)
}

fn keyed(event: &KeyEvent, held: Modifiers, pressed: bool, repeat: bool) -> Option<Event> {
    named(event.keysym).map(|key| Event::Key { key, physical_key: None, pressed, repeat, modifiers: held })
}

pub fn key_down(event: &KeyEvent, held: Modifiers, repeat: bool) -> Vec<Event> {
    [keyed(event, held, true, repeat), typed(event, held)].into_iter().flatten().collect()
}

pub fn key_up(event: &KeyEvent, held: Modifiers) -> Vec<Event> {
    keyed(event, held, false, false).into_iter().collect()
}
