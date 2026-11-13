mod app;
mod command;
mod control;
mod placement;
mod player;
mod plex;
mod presence;
mod shell;
mod store;
mod ui;
mod video;

use std::process::ExitCode;

use clap::Parser;

use command::Command;

#[derive(Parser)]
#[command(version, about = "A floating, click-through Plex picture-in-picture player")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

fn start() -> Result<(), String> {
    if control::already_running() {
        return Err("sth-plex-pip is already running. Use a subcommand to control it.".into());
    }
    let outcome = shell::run();
    control::close();
    outcome
}

fn main() -> ExitCode {
    let outcome = match Cli::parse().command {
        Some(command) => control::send(&command).map_err(|error| error.to_string()),
        None => start(),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
