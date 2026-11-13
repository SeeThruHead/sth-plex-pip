use std::fs;
use std::io;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::placement::Corner;

const HOME_OVERRIDE: &str = "STH_PLEX_PIP_HOME";
const APP: &str = "sth-plex-pip";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Auth {
    pub client_id: String,
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub corner: Corner,
    pub width: f32,
    pub opacity: f32,
    pub hover_opacity: f32,
    pub volume: f64,
    pub monitor: Option<String>,
    pub update_plex: bool,
    pub hotkeys: Hotkeys,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub controls: String,
    pub pause: String,
    pub back: String,
    pub forward: String,
    pub volume_up: String,
    pub volume_down: String,
    pub hide: String,
    pub dimmer: String,
    pub brighter: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            controls: "ctrl+alt+KeyP".into(),
            pause: "ctrl+alt+Space".into(),
            back: "ctrl+alt+ArrowLeft".into(),
            forward: "ctrl+alt+ArrowRight".into(),
            volume_up: "ctrl+alt+ArrowUp".into(),
            volume_down: "ctrl+alt+ArrowDown".into(),
            hide: "ctrl+alt+KeyH".into(),
            dimmer: "ctrl+alt+Minus".into(),
            brighter: "ctrl+alt+Equal".into(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            corner: Corner::BottomRight,
            width: 480.0,
            opacity: 1.0,
            hover_opacity: 0.12,
            volume: 100.0,
            monitor: None,
            update_plex: true,
            hotkeys: Hotkeys::default(),
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var_os(HOME_OVERRIDE)
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join(APP)))
        .unwrap_or_else(|| PathBuf::from(".").join(APP))
}

#[cfg(unix)]
pub fn runtime_dir() -> PathBuf {
    std::env::var_os(HOME_OVERRIDE).map(PathBuf::from).or_else(dirs::runtime_dir).unwrap_or_else(home)
}

fn read<T: DeserializeOwned>(name: &str) -> Option<T> {
    fs::read_to_string(home().join(name)).ok().and_then(|text| serde_json::from_str(&text).ok())
}

#[cfg(unix)]
fn private_file(path: &PathBuf) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn private_file(path: &PathBuf) -> io::Result<fs::File> {
    fs::File::create(path)
}

fn write<T: Serialize>(name: &str, value: &T) -> io::Result<()> {
    fs::create_dir_all(home())?;
    let file = private_file(&home().join(name))?;
    serde_json::to_writer_pretty(file, value).map_err(io::Error::other)
}

pub fn auth() -> Auth {
    read("auth.json").unwrap_or_else(|| {
        let fresh = Auth { client_id: uuid::Uuid::new_v4().to_string(), token: None };
        let _ = save_auth(&fresh);
        fresh
    })
}

pub fn save_auth(auth: &Auth) -> io::Result<()> {
    write("auth.json", auth)
}

pub fn settings() -> Settings {
    read("settings.json").unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> io::Result<()> {
    write("settings.json", settings)
}
