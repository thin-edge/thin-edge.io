#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
pub use unix::*;

#[cfg(not(windows))]
mod unix;

#[cfg(windows)]
mod windows;
