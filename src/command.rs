use serde::{Deserialize, Serialize};

use crate::placement::Corner;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, clap::Subcommand)]
pub enum Command {
    #[command(about = "Show or put away the controls (while shown, the window takes clicks)")]
    Controls,
    #[command(about = "Play or pause")]
    Pause,
    #[command(about = "Jump by this many seconds (negative goes back)")]
    Seek {
        #[arg(allow_negative_numbers = true)]
        seconds: f64,
    },
    #[command(about = "Change the volume by this much (negative is quieter)")]
    Volume {
        #[arg(allow_negative_numbers = true)]
        change: f64,
    },
    #[command(about = "Hide or show the window; hiding pauses")]
    Hide,
    #[command(about = "Change the window's opacity by this much, such as -0.1")]
    Opacity {
        #[arg(allow_negative_numbers = true)]
        change: f32,
    },
    #[command(about = "Move the window to a corner")]
    Corner { corner: Corner },
    #[command(about = "Play the first match for these words; with no words, resume the latest thing")]
    Play { words: Vec<String> },
    #[command(about = "Stop playback and go back to the library")]
    Stop,
    #[command(about = "Quit the player")]
    Quit,
}
