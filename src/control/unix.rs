use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

use crate::command::Command;
use crate::plex::worker::Wake;
use crate::store;

fn socket_path() -> PathBuf {
    store::runtime_dir().join("sth-plex-pip.sock")
}

pub fn send(command: &Command) -> io::Result<()> {
    let line = serde_json::to_string(command).map_err(io::Error::other)?;
    UnixStream::connect(socket_path())
        .and_then(|mut stream| writeln!(stream, "{line}"))
        .map_err(|error| io::Error::new(error.kind(), "sth-plex-pip is not running"))
}

pub fn close() {
    let _ = std::fs::remove_file(socket_path());
}

pub fn already_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

pub fn listen(commands: Sender<Command>, wake: Wake) -> io::Result<()> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    thread::spawn(move || {
        listener
            .incoming()
            .filter_map(Result::ok)
            .flat_map(|stream| BufReader::new(stream).lines().map_while(Result::ok))
            .filter_map(|line| serde_json::from_str::<Command>(&line).ok())
            .for_each(|command| {
                let _ = commands.send(command);
                wake();
            })
    });
    Ok(())
}
