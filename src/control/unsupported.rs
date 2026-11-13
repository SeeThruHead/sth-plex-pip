use std::io;
use std::sync::mpsc::Sender;

use crate::command::Command;
use crate::plex::worker::Wake;

pub fn send(_: &Command) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Controlling a running player from the command line is not available on Windows yet. Use the hotkeys.",
    ))
}

pub fn already_running() -> bool {
    false
}

pub fn listen(_: Sender<Command>, _: Wake) -> io::Result<()> {
    Ok(())
}
