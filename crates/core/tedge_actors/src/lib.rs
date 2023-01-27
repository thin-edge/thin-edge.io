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
//! #
//! /// State of the calculator actor
//! #[derive(Default)]
//! struct Calculator {
//!     state: i64,
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
//! ```
//!
//! ## Testing an actor
//!
//! To run and test an actor one needs to establish a bidirectional channel to its message box.
//! The simpler is to use the `Actor::MessageBox::channel()` function that creates two message boxes.
//! Along a message box ready to be used by the actor,
//! this function returns a second box connected to the former.
//! This message box can then be used to:
//! - send input messages to the actor
//! - receive output messages sent by the actor.
//!
//! ```
//! # use crate::tedge_actors::{Actor, ChannelError, MessageBox, SimpleMessageBox};
//! # use crate::tedge_actors::examples::calculator::*;
//! #
//! # #[tokio::main]
//! # async fn main() {
//! #
//! // Create a message box for the actor, along a test box ready to communicate with actor.
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
//! #
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
//! that can work with several clients (sending the responses to the appropriate requesters).
//!
//! ```
//! # use crate::tedge_actors::{Service, MessageBox, SimpleMessageBox};
//! # use async_trait::async_trait;
//!
//! # use crate::tedge_actors::examples;
//! # type Operation = examples::calculator::Operation;
//! # type Update = examples::calculator::Update;
//!
//! /// State of the calculator service
//! #[derive(Default)]
//! struct Calculator {
//!     state: i64,
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
//! ```
//!
//! A service can be tested directly through its `handle` method.
//! One can also build an actor, here a `ServiceActor<Calculator>`,
//! that uses the service implementation to serve requests.
//! This actor can then be tested using a test box connected to the actor box.
//!
//! ```
//! # use tedge_actors::{Actor, MessageBox, ServiceActor, SimpleMessageBox};
//! # use crate::tedge_actors::examples::calculator::*;
//! #
//! # #[tokio::main]
//! # async fn main_test() {
//! #
//! // As for any actor, one needs a bidirectional channel to the message box of the service.
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
//! The previous example, showing how to interact with a service using a test box,
//! only makes sense in the context of a test. One does not want to interact with an actor
//! through a *single* message box, that furthermore exposes internal details as client identifiers.
//! One must be free to connect several client actors to the same service actor,
//! and to connect a given actor to a bunch of peer actors delivering specific features.
//!
//! Let's start be implementing a client actor for the calculator service.
//!
//! ```
//! # use async_trait::async_trait;
//! # use tedge_actors::{Actor, ChannelError, MessageBox, RequestResponseHandler, ServiceActor, SimpleMessageBox};
//! # use crate::tedge_actors::examples::calculator::*;
//!
//! /// An actor that send operations to a calculator service to reach a given target.
//! struct Player {
//!     name: String,
//!     target: i64,
//! }
//!
//! #[async_trait]
//! impl Actor for Player {
//!
//!     /// This actor use a simple message box
//!     /// to receive `Update` messages and to send `Operation` messages.
//!     ///
//!     /// Presumably this actor interacts with a `Calculator`
//!     /// and will have to send an `Operation` before receiving in return an `Update`
//!     /// But nothing enforces that. The message box only tell what is sent and received.
//!     type MessageBox = SimpleMessageBox<Update,Operation>;
//!
//!     fn name(&self) -> &str {
//!         &self.name()
//!     }
//!
//!     async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
//!         // Send a first identity `Operation` to see where we are.
//!         messages.send(Operation::Add(0)).await?;
//!
//!         while let Some(status) = messages.recv().await {
//!             // Reduce by two the gap to the target
//!             let delta = self.target - status.to;
//!             messages.send(Operation::Add(delta / 2)).await?;
//!         }
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! To connect such an actor to the calculator, one needs message box builders
//! to establish appropriate connections between the actor message boxes.
//!
//! ```
//! # use tedge_actors::{Actor, Builder, ChannelError, MessageBox, MessageBoxPlug, NoConfig, ServiceActor, ServiceMessageBox, ServiceMessageBoxBuilder, SimpleMessageBox, SimpleMessageBoxBuilder};
//! # use crate::tedge_actors::examples::calculator::*;
//! # #[tokio::main]
//! # async fn main_test() -> Result<(),ChannelError> {
//! #
//!
//! // Building a box to hold 16 pending requests for the calculator service
//! // Note that a service actor requires a specific type of message box.
//! let mut service_box_builder = ServiceMessageBoxBuilder::new("Calculator", 16);
//!
//! // Building a box to hold one pending message for the player
//! // This actor never expect more then one message.
//! let mut player_1_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);
//!
//! // Connecting the two boxes, so the box built by the `player_box_builder`:
//! // - receives as input the messages sent by the box built by the `service_box_builder`
//! // - sends its output to the service input box.
//! player_1_box_builder.connect_to(&mut service_box_builder, NoConfig);
//!
//! // Its matters that the builder of the service box is a `ServiceMessageBoxBuilder`:
//! // this builder accept other actors to connect to the same service.
//! let mut player_2_box_builder = SimpleMessageBoxBuilder::new("Player 2", 1);
//! player_2_box_builder.connect_to(&mut service_box_builder, NoConfig);
//!
//! // One can then build the message boxes
//! let service_box: ServiceMessageBox<Operation,Update> = service_box_builder.build();
//! let mut player_1_box = player_1_box_builder.build();
//! let mut player_2_box = player_2_box_builder.build();
//!
//! // Then spawn the service
//! let service = Calculator::default();
//! tokio::spawn(ServiceActor::new(service).run(service_box));
//!
//! // And use the players' boxes to interact with the service.
//! // Note that, compared to the test above of the calculator service,
//! // - the players don't have to deal with client identifiers,
//! // - each player receives the responses to its requests,
//! // - the service processes the requests in the order they have been received,
//! // - the responses to a client are affected by the requests sent by the others.
//! player_1_box.send(Operation::Add(0)).await?;
//! player_2_box.send(Operation::Add(0)).await?;
//!
//! assert_eq!(player_1_box.recv().await, Some(Update{from:0,to:0}));
//! player_1_box.send(Operation::Add(10)).await?;
//!
//! assert_eq!(player_2_box.recv().await, Some(Update{from:0,to:0}));
//! player_2_box.send(Operation::Add(5)).await?;
//!
//! assert_eq!(player_1_box.recv().await, Some(Update{from:0,to:10}));
//! assert_eq!(player_2_box.recv().await, Some(Update{from:10,to:15}));
//! #
//! # Ok(())
//! # }
//! ```
//!
//! The previous example shown how to connect message boxes to an actor,
//! so one can use these message boxes to simulate actor peers.
//! However, it would be better to connect real peers
//! and then observe how the network of actors is behaving.
//!
//! Here, we interpose a `Probe` between two actors to observe their interactions.
//!
//! ```
//! # use tedge_actors::{Actor, Builder, ChannelError, MessageBoxPlug, NoConfig, ServiceActor, ServiceMessageBoxBuilder, SimpleMessageBoxBuilder};
//! # use tedge_actors::test_helpers::{MessageBoxPlugExt, Probe, ProbeEvent};
//! # use tedge_actors::test_helpers::ProbeEvent::{Recv, Send};
//! # use crate::tedge_actors::examples::calculator::*;
//! # #[tokio::main]
//! # async fn main_test() -> Result<(),ChannelError> {
//! #
//! // Build the actor message boxes
//! let mut service_box_builder = ServiceMessageBoxBuilder::new("Calculator", 16);
//! let mut player_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);
//!
//! // Connect the two actor message boxes interposing a probe.
//! let mut probe = Probe::new();
//! player_box_builder.with_probe(&mut probe).connect_to(&mut service_box_builder, NoConfig);
//!
//! // Spawn the actors
//! tokio::spawn(ServiceActor::new(Calculator::default()).run(service_box_builder.build()));
//! tokio::spawn(Player { name: "Player".to_string(), target: 42}.run(player_box_builder.build()));
//!
//! // Observe the messages sent and received by the player.
//! assert_eq!(probe.observe().await, Send(Operation::Add(0)));
//! assert_eq!(probe.observe().await, Recv(Update{from:0, to:0}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(21)));
//! assert_eq!(probe.observe().await, Recv(Update{from:0, to:21}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(10)));
//! assert_eq!(probe.observe().await, Recv(Update{from:21, to:31}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(5)));
//! assert_eq!(probe.observe().await, Recv(Update{from:31, to:36}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(3)));
//! assert_eq!(probe.observe().await, Recv(Update{from:36, to:39}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(1)));
//! assert_eq!(probe.observe().await, Recv(Update{from:39, to:40}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(1)));
//! assert_eq!(probe.observe().await, Recv(Update{from:40, to:41}));
//! assert_eq!(probe.observe().await, Send(Operation::Add(0)));
//! assert_eq!(probe.observe().await, Recv(Update{from:41, to:41}));
//! #
//! # Ok(())
//! # }
//! ```
//!
//! ## Using actor builders
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
mod errors;
pub mod keyed_messages;
pub mod message_boxes;
mod messages;
pub mod runtime;
mod tasks;

pub mod internal {
    pub use crate::tasks::*;
}
pub use actors::*;
pub use builders::*;
pub use channels::*;
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
