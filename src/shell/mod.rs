#[cfg(not(target_os = "linux"))]
mod desktop;
#[cfg(target_os = "linux")]
mod wayland;

use std::num::NonZeroU32;

#[cfg(not(target_os = "linux"))]
pub use desktop::run;
#[cfg(target_os = "linux")]
pub use wayland::run;

fn dimension(pixels: u32) -> NonZeroU32 {
    NonZeroU32::new(pixels).unwrap_or(NonZeroU32::MIN)
}
