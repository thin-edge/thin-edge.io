//! A library to define, compose and run actors
//!
//! Actors are processing units that interact using asynchronous messages.

mod actors;
mod channels;
mod errors;
mod messages;

pub use actors::*;
pub use channels::*;
pub use errors::*;
pub use messages::*;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
pub mod test_utils;
