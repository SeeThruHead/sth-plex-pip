use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_millis(40);

pub struct CursorProbe {
    socket: PathBuf,
}

pub fn parse(reply: &str) -> Option<(f32, f32)> {
    let (x, y) = reply.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

impl CursorProbe {
    pub fn find() -> Option<Self> {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
        let instance = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        let socket = PathBuf::from(runtime).join("hypr").join(instance).join(".socket.sock");
        socket.exists().then_some(Self { socket })
    }

    pub fn cursor(&self) -> Option<(f32, f32)> {
        let mut stream = UnixStream::connect(&self.socket).ok()?;
        stream.set_read_timeout(Some(PATIENCE)).ok()?;
        stream.set_write_timeout(Some(PATIENCE)).ok()?;
        stream.write_all(b"cursorpos").ok()?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply).ok()?;
        parse(&reply)
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_the_cursor_position_hyprland_prints() {
        assert_eq!(parse("1720, 811\n"), Some((1720.0, 811.0)));
        assert_eq!(parse("unknown request"), None);
    }
}
