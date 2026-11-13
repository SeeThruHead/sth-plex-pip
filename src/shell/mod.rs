#[cfg(not(target_os = "linux"))]
mod desktop;
#[cfg(target_os = "linux")]
mod wayland;

#[cfg(not(target_os = "linux"))]
pub use desktop::run;
#[cfg(target_os = "linux")]
pub use wayland::run;
