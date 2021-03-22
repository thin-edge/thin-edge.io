#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::*;

#[cfg(not(unix))]
mod windows;

#[cfg(not(unix))]
pub use windows::*;

pub const TEDGE_USER: &str = "tedge";
pub const BROKER_USER: &str = "mosquitto";

#[derive(thiserror::Error, Debug)]
pub enum UserSwitchError {
    #[error("Tried to become user, but it did not exist: {name}")]
    UnknownUser { name: String },

    #[error("Tried to become group, but it did not exist: {name}")]
    UnknownGroup { name: String },

    #[error(transparent)]
    NotAuthorized(#[from] std::io::Error),
}

