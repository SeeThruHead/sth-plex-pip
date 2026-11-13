#[cfg(unix)]
mod unix;
#[cfg(not(unix))]
mod unsupported;

#[cfg(unix)]
pub use unix::{already_running, close, listen, send};
#[cfg(not(unix))]
pub use unsupported::{already_running, close, listen, send};
