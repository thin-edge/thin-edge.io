//! A library to define, compose and run actors
//!
//! ## Implementing an actor
//!
//! Actors are processing units that interact using asynchronous messages.
//!
//! The behavior of an actor is defined by:
//! - a state freely defined and updated by the actor,
//! - input messages that the actor processes in turn,
//! - output messages produced by the actor.
//!
//! ```
//! # use crate::tedge_actors::{Actor, ChannelError, MessageBox, SimpleMessageBox};
//! # use async_trait::async_trait;
//!
//! /// State of the calculator actor
//! #[derive(Default)]
//! struct Calculator {
//!     state: i64,
//! }
//!
//! /// Input messages of the calculator actor
//! #[derive(Debug)]
//! enum Operation {
//!     Add(i64),
//!     Multiply(i64),
//! }
//!
//! /// Output messages of the calculator actor
//! #[derive(Debug, Eq, PartialEq)]
//! struct Update {
//!     from: i64,
//!     to: i64,
//! }
//!
//! /// Implementation of the calculator behavior
//! #[async_trait]
//! impl Actor for Calculator {
//!     // This actor uses a simple message box,
//!     // from where input messages are received
//!     // and to which output messages are sent.
//!     //
//!     // More sophisticated actors might used specific boxes,
//!     // notably to send and receive messages from specific peers.
//!     // However, this actor has no such needs: the input messages
//!     // are processed independently of their producers
//!     // and the output messages are sent independently of their consumers.
//!     type MessageBox = SimpleMessageBox<Operation,Update>;
//!
//!     fn name(&self) -> &str {
//!         "Calculator"
//!     }
//!
//!     async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
//!         while let Some(op) = messages.recv().await {
//!             // Process in turn each input message
//!             let from = self.state;
//!             let to = match op {
//!                Operation::Add(x) => from + x,
//!                Operation::Multiply(x) => from * x,
//!             };
//!
//!             // Update the actor state
//!             self.state = to;
//!
//!             // Send output messages
//!             messages.send(Update{from,to}).await?
//!         }
//!         Ok(())
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//!
//! // To run and test an actor one needs to establish a bidirectional channel to its message box.
//! // This message box will then be used to:
//! // - send input messages to the actor
//! // - receive output messages sent by the actor.
//! let (mut test_box, actor_box) = SimpleMessageBox::channel("Test", 10);
//!
//! // The actor is then spawn in the background with its message box.
//! let actor = Calculator::default();
//! tokio::spawn(actor.run(actor_box));
//!
//! // One can then interact with the actor
//! test_box.send(Operation::Add(4)).await.expect("message sent");
//! test_box.send(Operation::Multiply(10)).await.expect("message sent");
//! test_box.send(Operation::Add(2)).await.expect("message sent");
//!
//! // And observe its behavior
//! assert_eq!(test_box.recv().await, Some(Update{from:0,to:4}));
//! assert_eq!(test_box.recv().await, Some(Update{from:4,to:40}));
//! assert_eq!(test_box.recv().await, Some(Update{from:40,to:42}));
//!
//! # }
//! ```
//!
//! ## Deriving simple actors
//!
//! The `Actor` trait provides more flexibility than required by numerous actors.
//! This is notably the case for all actors that
//! - await for some input message, interpreted as a request,
//! - do something with that input, updating their state and possibly performing side effects,
//! - send back a single output message, interpreted as a response.
//!
//! Such an actor can be implemented as a `Service`.
//! Doing so, one can save some message box related code;
//! but the main benefit is that we can than build actors
//! that can work with several clients: sending the responses to the requesters.
//!
//! ```
//! # use crate::tedge_actors::{Service, MessageBox, SimpleMessageBox};
//! # use async_trait::async_trait;
//!
//! /// State of the calculator service
//! #[derive(Default)]
//! struct Calculator {
//!     state: i64,
//! }
//!
//! /// Input messages of the calculator service
//! #[derive(Debug)]
//! enum Operation {
//!     Add(i64),
//!     Multiply(i64),
//! }
//!
//! /// Output messages of the calculator service
//! #[derive(Debug, Eq, PartialEq)]
//! struct Update {
//!     from: i64,
//!     to: i64,
//! }
//!
//! /// Implementation of the calculator behavior
//! #[async_trait]
//! impl Service for Calculator {
//!
//!     type Request = Operation;
//!     type Response = Update;
//!
//!     fn name(&self) -> &str {
//!         "Calculator"
//!     }
//!
//!     async fn handle(&mut self, request: Self::Request) -> Self::Response {
//!         // Act accordingly to the request
//!         let from = self.state;
//!         let to = match request {
//!            Operation::Add(x) => from + x,
//!            Operation::Multiply(x) => from * x,
//!         };
//!
//!         // Update the service state
//!         self.state = to;
//!
//!         // Return the response
//!         Update{from,to}
//!     }
//! }
//!
//! # use tedge_actors::{Actor, ServiceActor};
//! # #[tokio::main]
//! # async fn main_test() {
//!
//! // As for any actor, one needs a bidirectional channel to the message box of the service.
//!
//! let (mut test_box, actor_box) = SimpleMessageBox::channel("Test", 10);
//!
//! // The actor is then spawn in the background with its message box.
//! let service = Calculator::default();
//! let actor = ServiceActor::new(service);
//! tokio::spawn(actor.run(actor_box));
//!
//! // One can then interact with the actor
//! // Note that now each request is prefixed by a number: the id of the requester
//! test_box.send((1,Operation::Add(4))).await.expect("message sent");
//! test_box.send((2,Operation::Multiply(10))).await.expect("message sent");
//! test_box.send((1,Operation::Add(2))).await.expect("message sent");
//!
//! // Observing the service behavior,
//! // note that the responses come back associated to the id of the requester.
//! assert_eq!(test_box.recv().await, Some((1,Update{from:0,to:4})));
//! assert_eq!(test_box.recv().await, Some((2,Update{from:4,to:40})));
//! assert_eq!(test_box.recv().await, Some((1,Update{from:40,to:42})));
//!
//! # }
//! ```
//!
//! ## Connecting actors
//!
//! Actors don't work in isolation.
//! They interact by sending messages and a key step is to connect the actors to each others,
//! or, more precisely, to connect their message boxes.
//!
//! TODO
//!
//! ## Implementing specific message boxes
//!
//! TODO
//!
#![forbid(unsafe_code)]

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

pub mod internal {
    pub use crate::tasks::*;
}
pub use actors::*;
pub use builders::*;
pub use channels::*;
pub use errors::*;
pub use handles::*;
pub use keyed_messages::*;
pub use message_boxes::*;
pub use messages::*;
pub use runtime::*;

pub use futures::channel::mpsc;
pub use futures::SinkExt;
pub use futures::StreamExt;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
pub mod tests;
