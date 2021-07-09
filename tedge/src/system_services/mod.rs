//! Common interface to the system-provided _service management facility_ to start, stop, enable,
//! disable or query the status of system services.
//!
//! Supported service manangement facilities include:
//!
//! * systemd
//! * OpenRC
//! * `service(8)` as found on BSDs.
//!

mod error;
mod manager;
mod manager_ext;
mod managers;
mod services;

pub use self::{error::*, manager::*, manager_ext::*, managers::*, services::*};
