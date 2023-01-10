//! A library to define, compose and run actors
//!
//! Actors are processing units that interact using asynchronous messages.

mod actors;
mod builders;
mod channels;
mod errors;
mod handles;
mod keyed_messages;
mod message_boxes;
mod messages;
mod runtime;
mod tasks;

pub use actors::*;
pub use builders::*;
pub use channels::*;
pub use errors::*;
pub use handles::*;
pub use keyed_messages::*;
pub use message_boxes::*;
pub use messages::*;
pub use runtime::*;
pub use tasks::*;

pub use futures::channel::mpsc;
pub use futures::SinkExt;
pub use futures::StreamExt;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
pub mod tests;

#[cfg(test)]
pub mod test_utils;
