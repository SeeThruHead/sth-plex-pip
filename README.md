# sth-plex-pip

A small Plex player that floats in a corner of the screen, on top of everything, and stays out of the way. It is for watching something while you work or play a game.

- Plays the original file with mpv (libmpv), so MKV, HEVC, HDR and EAC3 play as they are instead of being converted by the server.
- Always on top, also above full-screen windows and games. Tiling window managers leave it alone.
- Never takes clicks or keyboard focus while you watch. Move the pointer over it and it fades so you can see and click what is underneath.
- A hotkey brings up controls: seek, pause, volume, audio and subtitle tracks, drag to another corner, resize.
- Signs in with your Plex account like any other Plex client, resumes where you left off, and reports progress back so Plex stays in step. There is no server of its own.

## Install

macOS and Linux:

```sh
brew install seethruhead/tap/sth-plex-pip
```

On Linux the player uses the system's mpv library (`libmpv.so.2`, from the `mpv` or `libmpv2` package). On macOS brew installs mpv with it. For Windows, download the zip from the releases page and keep `libmpv-2.dll` next to the program.

## Use

Run `sth-plex-pip`. The first time it shows "Sign in with Plex" and opens the Plex sign-in page in your browser. After that it lists what you were watching and your libraries. Pick something and it plays in the corner.

While something plays, the window ignores the mouse. Everything is done with hotkeys or commands:

| Command | What it does |
|---|---|
| `sth-plex-pip controls` | Show or put away the controls. While shown, the window takes clicks. |
| `sth-plex-pip pause` | Play or pause |
| `sth-plex-pip seek 30` / `seek -10` | Jump forward or back, in seconds |
| `sth-plex-pip volume 5` / `volume -5` | Louder or quieter |
| `sth-plex-pip hide` | Hide (and pause) or show the window |
| `sth-plex-pip opacity -0.1` | Make the window more see-through; `0.1` less |
| `sth-plex-pip corner top-left` | Move to a corner |
| `sth-plex-pip play the matrix` | Play the first match; with no words, resume the latest thing |
| `sth-plex-pip stop` | Stop and go back to the library |
| `sth-plex-pip quit` | Quit |

### Hotkeys

macOS and Windows register global hotkeys themselves. The defaults are Ctrl+Alt (Ctrl+Option on a Mac) with P (controls), Space (pause), Left and Right (seek), Up and Down (volume), H (hide), Minus and Equal (opacity). Holding Ctrl+Alt while the pointer is over the window also shows the controls. Change them in `settings.json`.

Wayland has no global hotkeys for applications, so on Linux you bind keys in the compositor to the commands above. Hyprland (Lua config):

```lua
hl.bind("CTRL + ALT + P", hl.dsp.exec_cmd("sth-plex-pip controls"))
hl.bind("CTRL + ALT + SPACE", hl.dsp.exec_cmd("sth-plex-pip pause"))
hl.bind("CTRL + ALT + Right", hl.dsp.exec_cmd("sth-plex-pip seek 30"))
hl.bind("CTRL + ALT + Left", hl.dsp.exec_cmd("sth-plex-pip seek -10"))
hl.bind("CTRL + ALT + H", hl.dsp.exec_cmd("sth-plex-pip hide"))
```

### Settings

`settings.json` and `auth.json` live in the config folder (`~/.config/sth-plex-pip` on Linux, `~/Library/Application Support/sth-plex-pip` on macOS, `%APPDATA%\sth-plex-pip` on Windows). `auth.json` holds the Plex token and is readable only by you.

| Setting | Meaning | Default |
|---|---|---|
| `corner` | `top-left`, `top-right`, `bottom-left`, `bottom-right` | `bottom-right` |
| `width` | Width of the video window | `480` |
| `opacity` | Opacity while watching | `1.0` |
| `hover_opacity` | Opacity while the pointer is over it | `0.12` |
| `volume` | 0 to 100 | `100` |
| `update_plex` | Report progress and watched state to Plex | `true` |
| `hotkeys` | The global hotkeys (macOS and Windows) | see above |

Set `STH_PLEX_PIP_HOME` to a folder to run a separate instance with its own sign-in and settings. Set `STH_PLEX_PIP_MPV_LOG` to a file path to have mpv write its log there, which shows for example whether hardware decoding is in use.

## What each system allows

- **Linux**: needs a Wayland compositor with layer-shell (Hyprland, Sway, KDE Plasma, niri and others). GNOME has none, so it does not run there. The overlay sits above full-screen windows, including games under Proton. Fading under the pointer needs to know where the pointer is while clicks pass through, which Wayland does not tell applications. On Hyprland the player asks the compositor and fades at once. Elsewhere it checks about twice a second by taking the pointer for an instant, so the fade lags by up to half a second.
- **macOS**: floats above full-screen apps and on every Space. While you watch it never becomes the active app; clicking its controls (after the hotkey) does make it active, like any window you click.
- **Windows**: floats above borderless and windowed full-screen games. A game in exclusive full screen owns the display and nothing can draw over it; set the game to borderless or windowed full screen (World of Warcraft has no exclusive mode any more). The Windows build is compiled in CI and has not been tried on a real machine yet.
- **Dolby Vision**: files with an HDR10 base layer play as HDR, tone-mapped to the screen. mpv's embedded renderer does not do Dolby Vision's own colour mapping, so profile 5 files (no HDR10 layer) show wrong colours.

## Build

Rust and libmpv's development files are needed (`mpv` on Arch and brew, `libmpv-dev` on Debian and Ubuntu).

```sh
cargo build --release
cargo test
```
