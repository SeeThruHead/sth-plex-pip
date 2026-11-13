#[cfg(unix)]
mod unix;
#[cfg(not(unix))]
mod unsupported;

#[cfg(unix)]
pub use unix::{already_running, listen, send};
#[cfg(not(unix))]
pub use unsupported::{already_running, listen, send};
