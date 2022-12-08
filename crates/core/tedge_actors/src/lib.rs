//! A library to define, compose and run actors
//!
//! Actors are processing units that interact using asynchronous messages.

mod actors;
mod builders;
mod channels;
mod errors;
mod keyed_messages;
mod messages;
mod runtime;
mod tasks;

pub use actors::*;
pub use builders::*;
pub use channels::*;
pub use errors::*;
pub use keyed_messages::*;
pub use messages::*;
pub use runtime::*;
pub use tasks::*;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
pub mod test_utils;
