use std::str::FromStr;
use std::sync::mpsc::Sender;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::command::Command;
use crate::plex::worker::Wake;
use crate::store::Hotkeys;

fn bindings(keys: &Hotkeys) -> Vec<(&str, Command)> {
    vec![
        (keys.controls.as_str(), Command::Controls),
        (keys.pause.as_str(), Command::Pause),
        (keys.back.as_str(), Command::Seek { seconds: -10.0 }),
        (keys.forward.as_str(), Command::Seek { seconds: 30.0 }),
        (keys.volume_up.as_str(), Command::Volume { change: 5.0 }),
        (keys.volume_down.as_str(), Command::Volume { change: -5.0 }),
        (keys.hide.as_str(), Command::Hide),
        (keys.dimmer.as_str(), Command::Opacity { change: -0.1 }),
        (keys.brighter.as_str(), Command::Opacity { change: 0.1 }),
    ]
}

pub fn register(keys: &Hotkeys, commands: Sender<Command>, wake: Wake) -> Option<GlobalHotKeyManager> {
    let manager = GlobalHotKeyManager::new().ok()?;
    let registered: Vec<(u32, Command)> = bindings(keys)
        .into_iter()
        .filter_map(|(text, command)| HotKey::from_str(text).ok().map(|hotkey| (hotkey, command)))
        .filter(|(hotkey, _)| manager.register(*hotkey).is_ok())
        .map(|(hotkey, command)| (hotkey.id(), command))
        .collect();
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state != HotKeyState::Pressed {
            return;
        }
        registered
            .iter()
            .filter(|(id, _)| *id == event.id)
            .for_each(|(_, command)| {
                let _ = commands.send(command.clone());
                wake();
            });
    }));
    Some(manager)
}
