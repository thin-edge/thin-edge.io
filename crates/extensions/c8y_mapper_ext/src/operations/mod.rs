//! Utilities for executing Cumulocity operations.
//!
//! C8y operations need some special handling by the C8y mapper, which needs to use Smartrest via
//! MQTT or C8y HTTP proxy to report on their progress. Additionally, while executing operations we
//! often need to send messages to different actors and wait for their results before continuing.
//!
//! The operations are always triggered remotely by Cumulocity, and a triggered operation must
//! always terminate in a success or failure. This status needs to be reported to Cumulocity.
//!
//! This module contains:
//! - operation handler, which handles thin-edge operation MQTT messages by spawning tasks that
//!   handle different operations ([`handler`])
//! - conversion from C8y operation messages into thin-edge operation messages ([`convert`])
//! - implementations of operations ([`handlers`])
//!
//! thin-edge.io operations reference:
//! https://thin-edge.github.io/thin-edge.io/operate/c8y/supported-operations/

mod convert;
mod error;

mod handler;
pub use handler::OperationHandler;

mod handlers;
pub use handlers::EntityTarget;

mod upload;

mod actor;
mod builder;
pub use builder::OperationHandlerBuilder;

mod c8y_operations;
