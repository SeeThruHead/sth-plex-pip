# sth-plex-pip

A floating, click-through Plex player. Rust, libmpv for playback, egui for the interface, drawn with OpenGL.

## Commands

- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`: run all three before every push. CI runs the same on Linux, macOS and Windows.
- `cargo build --release` builds the one binary.
- Try a change in its own instance: `STH_PLEX_PIP_HOME=<folder> cargo run`. Put `{"volume":0,"update_plex":false}` in that folder's `settings.json` so a test makes no sound and leaves Plex's watch state alone.

## How it starts

`main.rs` parses the command line. With a subcommand it sends that command to the running player over a Unix socket (`control/`) and exits. With none it calls `shell::run()`, which opens the window for this system and runs the event loop until quit.

## Layout

- `plex/`: the Plex client. `model.rs` is what Plex sends, `client.rs` is the HTTP calls (sign-in PIN, server discovery, library, progress), `worker.rs` runs them on a thread so the window never waits on the network.
- `player.rs`: mpv. Loading, seeking, tracks, and drawing a frame into a framebuffer.
- `video.rs`: the texture mpv draws into, which egui then paints.
- `presence.rs`: pure. From "is it playing, is the pointer over it, were controls asked for" to "how opaque, does it take clicks, are controls shown".
- `placement.rs`: pure. Corners, snapping, resizing and drag maths.
- `ui.rs`: the egui views. Views return a list of `Action`s; they change nothing themselves.
- `app.rs`: owns the state and ties it together. One `frame()` per redraw: handle commands, Plex replies and player events, draw, and return what the window should look like (`Wants`).
- `shell/`: the window, one per system. `wayland/` is a layer-shell overlay; `desktop/` is winit for macOS and Windows with a small file each for what winit does not cover. A shell feeds input to `App::frame` and applies the `Wants` it gets back.
- `store.rs`: settings and sign-in files. `command.rs`: the commands shared by the CLI, the socket and the hotkeys.

## Rules

- No loops, no comments, guard clauses instead of nested ifs. Mutation only where the toolkit's API needs it.
- Anything that differs by system lives in `shell/` or behind a `cfg` in `control/` and `store.rs`. Nothing else names an operating system.
- `presence.rs` and `placement.rs` stay pure and tested; window behaviour is decided there, not in a shell.
- Tests assert what a user would see (where the window lands, whether it takes clicks), never how the code got there.
- The Plex token never goes in a URL, a log line or argv. mpv gets it as a request header.
- No AI attribution in commits, docs or metadata.

## What reviewers check

- A new window behaviour is a field on `Presence` or `Wants`, handled by every shell, not a special case inside one.
- New Plex calls go through `worker.rs`; nothing on the window thread touches the network.
- Every external wait has a timeout.
