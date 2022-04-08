#[cfg(feature = "async-tokio")]
mod logged_command_async;

#[cfg(feature = "async-tokio")]
pub use crate::logged_command_async::{LoggedCommand, LoggingChild};

#[cfg(feature = "sync-std")]
mod logged_command_sync;

#[cfg(feature = "sync-std")]
pub use crate::logged_command_sync::{LoggedCommand, LoggingChild};
