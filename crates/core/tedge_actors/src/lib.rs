//! A library to define, compose and run actors
//!
//! ## Implementing an actor
//!
//! Actors are processing units that interact using asynchronous messages.
//!
//! The behavior of an actor is defined by:
//! - the state owned and freely updated by the actor,
//! - a message box connected to peer actors,
//! - input messages that the actor receives from its peers and processes in turn,
//! - output messages that the actor produces and sends to its peers.
//!
//! ```
//! # use crate::tedge_actors::{Actor, RuntimeError, MessageReceiver, RuntimeRequest, Sender, SimpleMessageBox};
//! # use async_trait::async_trait;
//! #
//! /// State of the calculator actor
//! struct Calculator {
//!     state: i64,
//!     messages: SimpleMessageBox<Operation, Update>,
//! }
//!
//! /// Input messages of the calculator actor
//! #[derive(Debug, Eq, PartialEq)]
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
//!
//!     fn name(&self) -> &str {
//!         "Calculator"
//!     }
//!
//!     async fn run(&mut self)-> Result<(), RuntimeError>  {
//!         while let Some(op) = self.messages.recv().await {
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
//!             self.messages.send(Update{from,to}).await?
//!         }
//!         Ok(())
//!     }
//! }
//! ```
//!
//! The `Actor` trait provides the flexibility to:
//!
//! - use a specific [MessageBox](crate::MessageBox) implementation
//!   to address specific communication needs
//!   (pub/sub, request/response, message priority, concurrent message processing, ...)
//! - freely interleave message reception and emission in its [Actor::run()](crate::Actor::run) event loop,
//!   reacting to peer messages as well as internal events,
//!   sending responses for requests, possibly deferring some responses,
//!   acting as a source of messages ...
//!
//! This crate also provides specific `Actor` implementations:
//! - The [ServerActor](crate::ServerActor) wraps a [Server](crate::Server),
//!   to implement a request-response communication pattern with a set of connected client actors.
//!
//! ## Testing an actor
//!
//! To run and test an actor one needs to create a test message box connected to the actor message box.
//! This test box can then be used to:
//! - send input messages to the actor
//! - receive output messages sent by the actor.
//!
//! ```
//! # use crate::tedge_actors::{Actor, ChannelError, MessageReceiver, Sender, SimpleMessageBox};
//! # use crate::tedge_actors::examples::calculator::*;
//! #
//! # #[tokio::main]
//! # async fn main() {
//! #
//! // Create a message box for the actor, along a test box ready to communicate with the actor.
//! use tedge_actors::{Builder, NoConfig, SimpleMessageBoxBuilder};
//! use tedge_actors::test_helpers::ServiceProviderExt;
//! let mut actor_box_builder = SimpleMessageBoxBuilder::new("Actor", 10);
//! let mut test_box = actor_box_builder.new_client_box(NoConfig);
//! let actor_box = actor_box_builder.build();
//!
//! // The actor is then spawn in the background with its message box.
//! let mut actor = Calculator::new(actor_box);
//! tokio::spawn(async move { actor.run().await } );
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
//! #
//! # }
//! ```
//!
//! See the [test_helpers](crate::test_helpers) module for various ways
//! to observe and interact with running actors.
//!
//! - A [Probe](crate::test_helpers::Probe) can be interleaved between two actors
//!   to observe their interactions.
//!
//! ## Connecting actors
//!
//! Actors don't work in isolation.
//! They interact by sending messages and a key step is to connect the actors with each other.
//!
//! Connection between actors are established using [actor and message box builders](crate::builders).
//! These builders implement connector traits
//! that define the services provided and consumed by the actors under construction.
//!
//! The connection builder traits work as pairs:
//! - An actor that provides some service makes this service available
//!   with
//! an actor builder that implements the [ServiceProvider](crate::ServiceProvider) trait.
//! - In a symmetrical way, an actor that requires another service to provide its own feature,
//!   implements the [ServiceConsumer](crate::ServiceConsumer) trait
//!   to connect itself to the [ServiceProvider](crate::ServiceProvider)
//! - These two traits define the types of the messages sent in both directions
//!   and how to connect the message boxes of the actors under construction,
//!   possibly using some configuration.
//! - Two actor builders, a `consumer: ServiceConsumer<I,O,C>` and a `producer: ServiceProvider<I,O,C>`
//!   can then be connected to each other : `consumer.set_connection(producer)`
//!
//! ```no_run
//! # use tedge_actors::{DynSender, NoConfig, ServiceConsumer, ServiceProvider};
//! # #[derive(Default)]
//! # struct SomeActorBuilder;
//! # #[derive(Default)]
//! # struct SomeOtherActorBuilder;
//! # impl ServiceProvider<(),(),NoConfig> for SomeActorBuilder {
//! #     fn connect_consumer(&mut self, config: NoConfig, response_sender: DynSender<()>) -> DynSender<()> {
//! #         todo!()
//! #     }
//! # }
//! #
//! # impl ServiceConsumer<(),(),NoConfig> for SomeOtherActorBuilder {
//! #     fn get_config(&self) -> NoConfig {
//! #        todo!()
//! #     }
//! #     fn set_request_sender(&mut self, request_sender: DynSender<()>) {
//! #         todo!()
//! #     }
//! #     fn get_response_sender(&self) -> DynSender<()> {
//! #         todo!()
//! #     }
//! # }
//!
//! // An actor builder declares that it provides a service
//! // by implementing the `ServiceProvider` trait for the appropriate input, output and config types.
//! //
//! // Here, `SomeActorBuilder: ServiceProvider<SomeInput, SomeOutput, SomeConfig>`
//! let mut producer = SomeActorBuilder::default();
//!
//! // An actor builder also declares that it is a consumer of other services required by it. This is done
//! // by implementing the `ServiceConsumer` trait for the appropriate input, output and config types.
//! //
//! // Here, `SomeOtherActorBuilder: ServiceConsumer<SomeInput, SomeOutput, SomeConfig>`
//! let mut consumer = SomeOtherActorBuilder::default();
//!
//! // These two actors having compatible expectations along input, output and config types,
//! // can then be connected to each other.
//! consumer.set_connection(&mut producer);
//! ```
//!
//! ## Running actors
//!
//! TODO
//!
//! ## Implementing specific message boxes
//!
//! TODO
//!
#![forbid(unsafe_code)]

mod actors;
pub mod builders;
pub mod channels;
mod converter;
mod errors;
pub mod keyed_messages;
pub mod message_boxes;
mod messages;
mod run_actor;
pub mod runtime;

pub mod internal {
    pub use crate::run_actor::*;
}
pub use actors::*;
pub use builders::*;
pub use channels::*;
pub use converter::*;
pub use errors::*;
pub use keyed_messages::*;
pub use message_boxes::*;
pub use messages::*;
pub use runtime::*;

pub use futures;
use futures::channel::mpsc;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
pub mod tests;

// FIXME: how to have these examples only available when testing the doc comments?
// #[cfg(test)]
#[doc(hidden)]
pub mod examples;

pub mod test_helpers;
