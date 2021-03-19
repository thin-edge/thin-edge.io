#[cfg(unix)]
pub mod unix;

#[cfg(unix)]
pub use unix::*;

#[cfg(not(unix))]
pub mod windows;

#[cfg(not(unix))]
pub use windows::*;
