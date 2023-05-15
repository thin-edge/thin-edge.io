//! A library to define, compose and run actors
//!
//! ## Implementing an actor
//!
//! Actors are processing units that interact using asynchronous messages.
//!
//! The behavior of an [Actor](crate::Actor) is defined by:
//! - a state owned and freely updated by the actor,
//! - a [message box](crate::message_boxes) connected to peer actors,
//! - input [messages](crate::Message) that the actor receives from its peers and processes in turn,
//! - output [messages](crate::Message) that the actor produces and sends to its peers,
//! - an event loop, the [Actor::run()](crate::Actor::run) method.
//!
//!
//! ```
//! # use crate::tedge_actors::{Actor, RuntimeError, MessageReceiver, RuntimeRequest, Sender, SimpleMessageBox};
//! # use async_trait::async_trait;
//! #
//! /// State of the calculator actor along with its message box
//! struct Calculator {
//!     /// The actor state. Here a simple number.
//!     ///
//!     /// This state rules the behavior of the actor
//!     state: i64,
//!
//!     /// This actor uses a simple message box,
//!     /// from where input messages are received
//!     /// and to which output messages are sent.
//!     ///
//!     /// More sophisticated actors might use specific boxes,
//!     /// notably to send and receive messages from specific peers.
//!     /// However, this actor has no such needs: the input messages
//!     /// are processed independently of their producers
//!     /// and the output messages are sent independently of their consumers.
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
//!     /// The actor name is only used for logging
//!     fn name(&self) -> &str {
//!         "Calculator"
//!     }
//!
//!     /// Run the actor: processing message in, sending message out, updating the internal state
//!     ///
//!     /// This actor implements a simple message loop:
//!     /// - input messages are processed in turn,
//!     /// - output messages are sent to respond to some input.
//!     ///
//!     /// However, there are no constraints on the behavior of an actor.
//!     /// A more sophisticated actor might send output messages independently of any request
//!     /// or process concurrently several requests.
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
//! This crate provides specific `Actor` implementations:
//! - The [ServerActor](crate::ServerActor) wraps a [Server](crate::Server),
//!   to implement a request-response communication pattern with a set of connected client actors.
//! - The [ConvertingActor](crate::ConvertingActor) wraps a [Converter](crate::Converter),
//!   that translates each input message into a sequence of output messages.
//!
//! ## Testing an actor
//!
//! To test an actor no specific actor runtime is required.
//! One just needs to create a test message boxes connected to the actor message box,
//! in order to interact with the running actor
//! by sending input messages and checking the output messages.
//!
//! As each actor is free to chose its own implementation for its message box,
//! the details on how to connect test message boxes will be specific to each actor.
//! [Actor and message box builders](crate::builders) are provided to address these specificities
//! with a generic approach without exposing the internal structure of the actors.
//!
//! To test the `Calculator` example we need first to create its box using a
//! [SimpleMessageBoxBuilder](crate::SimpleMessageBoxBuilder),
//! as this actor expects a [SimpleMessageBox](crate::SimpleMessageBox).
//! And then, to create a test box connected to the actor message box,
//! we use the [ServiceProviderExt](crate::test_helpers::ServiceProviderExt) test helper extension
//! and the [new_client_box](crate::test_helpers::ServiceProviderExt::new_client_box) method.
//!
//! ```
//! # use crate::tedge_actors::Actor;
//! # use crate::tedge_actors::Builder;
//! # use crate::tedge_actors::ChannelError;
//! # use crate::tedge_actors::MessageReceiver;
//! # use crate::tedge_actors::NoConfig;
//! # use crate::tedge_actors::Sender;
//! # use crate::tedge_actors::SimpleMessageBox;
//! # use crate::tedge_actors::SimpleMessageBoxBuilder;
//! # use crate::tedge_actors::examples::calculator::*;
//! #
//! #[cfg(feature = "test-helpers")]
//! # #[tokio::main]
//! # async fn main() {
//! #
//! // Add the `new_client_box()` extension to the `SimpleMessageBoxBuilder`.
//! use tedge_actors::test_helpers::ServiceProviderExt;
//!
//! // Use a builder for the actor message box
//! let mut actor_box_builder = SimpleMessageBoxBuilder::new("Actor", 10);
//!
//! // Create a test box ready then the actor box
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
//! - The primary tool to interact with an actor under test is the [SimpleMessageBoxBuilder],
//!   that can be used to connect [SimpleMessageBox] and interact with the actor.
//! - The [MessageReceiverExt](crate::test_helpers::MessageReceiverExt) extension
//!   extends a message with assertion methods checking that expected messages are actually received
//!   .i.e sent by the actor under test.
//! - The [ServiceProviderExt](crate::test_helpers::ServiceProviderExt) extension
//!   extends the message box builders of any actor that [provide a service](crate::ServiceProvider)
//! - The [ServiceConsumerExt](crate::test_helpers::ServiceConsumerExt) extension
//!   extends the message box builders of any actor that [consume a service](crate::ServiceConsumer)
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
//! The connection builder traits work by pairs:
//! - A [MessageSink](crate::MessageSink) connects to a [MessageSource](crate::MessageSource),
//!   so the messages sent by the latter will be received by the former.
//! - A [ServiceConsumer](crate::ServiceConsumer) connects a [ServiceProvider](crate::ServiceProvider),
//!   to use the service, sending requests to and receiving responses from the service.
//!
//! These traits define the types of the messages sent and received.
//! - A sink that excepts message of type `M` can only be connected to a source of messages
//!   that can be converted into `M` values.
//! - Similarly a service is defined by two types of messages, the requests received by the service
//!   and the responses sent by the service. To use a service, a consumer will have to send messages
//!   that can be converted into the service request type and be ready to receive messages converted from
//!   the service response type.
//! - Note, that no contract is enforced beyond the type-compatibility of the messages sent between the actors.
//!   A consumer of an HTTP service needs to known that a request must be sent before any response can be received;
//!   while a consumer of an MQTT service can expect to receive messages without sending a single one.
//!
//! The connection builder traits also define a configuration type.
//! - The semantics of this type is defined by the message source or the service provider.
//!   It can be used to filter the values sent to a given sink
//!   or to restrict the scope of the service provided to a given service consumer.
//! - The configuration values are provided by the message sinks and the service consumers
//!   to specify the context of their connection to a source or a service.
//!
//! Note that these traits are implemented by the actor builders, not by the actors themselves.
//!
//! ```no_run
//! # use tedge_actors::{DynSender, NoConfig, ServiceConsumer, ServiceProvider};
//! # #[derive(Default)]
//! # struct SomeActorBuilder;
//! # #[derive(Default)]
//! # struct SomeOtherActorBuilder;
//! # #[derive(Debug)]
//! # struct SomeInput;
//! # #[derive(Debug)]
//! # struct SomeOutput;
//! # struct SomeConfig;
//! /// An actor builder declares that it provides a service
//! /// by implementing the `ServiceProvider` trait for the appropriate input, output and config types.
//! impl ServiceProvider<SomeInput,SomeOutput,SomeConfig> for SomeActorBuilder {
//!     /// Exchange two message senders with the new peer, so each can send messages to the other
//!     ///
//!     /// The service registers the new consumer and its sender (i.e. where to send response),
//!     /// possibly using the configuration `config` to adapt the service,
//!     /// and returns to the consumer a sender where the requests will have to be sent.
//!     fn connect_consumer(&mut self, config: SomeConfig, response_sender: DynSender<SomeOutput>)
//!         -> DynSender<SomeInput> {
//!          todo!()
//!      }
//! }
//!
//! /// An actor builder also declares that it is a consumer of other services required by it. This is done
//! /// by implementing the `ServiceConsumer` trait for the appropriate input, output and config types.
//! impl ServiceConsumer<SomeInput,SomeOutput,SomeConfig> for SomeOtherActorBuilder {
//!     fn get_config(&self) -> SomeConfig {
//!        todo!()
//!     }
//!
//!     /// Update this actor with the sender where the service expects input messages to be sent
//!     fn set_request_sender(&mut self, request_sender: DynSender<SomeInput>) {
//!         todo!()
//!     }
//!
//!     /// Tell the service where to send its output messages to this actor
//!     fn get_response_sender(&self) -> DynSender<SomeOutput> {
//!         todo!()
//!     }
//! }
//!
//! // These two actors having compatible expectations along input, output and config types,
//! // can then be connected to each other.
//! let mut producer = SomeActorBuilder::default();
//! let mut consumer = SomeOtherActorBuilder::default();
//! consumer.set_connection(&mut producer);
//! ```
//!
//! ## Running actors
//!
//! An [Actor] can [run](crate::Actor::run) without any specific runtime.
//! However, running the actors of an application in the context of the [tedge_actors::Runtime](crate::Runtime)
//! has several benefits:
//! - The runtime monitors all the running actors, catching normal terminations, aborts and panics.
//! - The runtime can send [RuntimeRequest] to all the running actors,
//!   notably to trigger a graceful shutdown of the application.
//! - Any actor can send [RuntimeAction] to the runtime,
//!   to spawn a new actor or to request a global shutdown of the application.
//! - An actor can subscribe to the [RuntimeEvent] published by the runtime,
//!   to be notified of actor events such as start, termination or crash.
//!
//! To run an actor `A` using the [tedge_actors::Runtime](crate::Runtime) requires more than just
//! an [Actor] implementation. One needs an [actor builder](crate::builders) that implements:
//! - `Builder<A>` to let the runtime create the actor instance,
//! - `RuntimeRequestSink` so the [Runtime] can be connected to the runtime,
//! - possibly [MessageSink], [MessageSource], [ServiceProvider] or [ServiceConsumer],
//!   to be connected to other actors, accordingly to the actor dependencies and services.
//!
//! ```no_run
//! # use std::convert::Infallible;
//! # use tedge_actors::{Actor, Builder, DynSender, Runtime, RuntimeError, RuntimeRequest, RuntimeRequestSink};
//! struct MyActor;
//! # #[derive(Default)]
//! struct MyActorBuilder;
//!
//! #[async_trait::async_trait]
//! impl Actor for MyActor {
//!    fn name(&self) -> &str {
//!         todo!()
//!     }
//!
//!     async fn run(&mut self) -> Result<(), RuntimeError> {
//!         todo!()
//!     }
//! }
//!
//! impl Builder<MyActor> for MyActorBuilder {
//!     type Error = Infallible;
//!
//!     fn try_build(self) -> Result<MyActor, Self::Error> {
//!         todo!()
//!     }
//! }
//!
//! impl RuntimeRequestSink for MyActorBuilder {
//!     fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
//!        todo!()
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), RuntimeError> {
//! let runtime_events_logger = None;
//! let mut runtime = Runtime::try_new(runtime_events_logger).await?;
//!
//! let my_actor_builder = MyActorBuilder::default();
//!
//! runtime.spawn(my_actor_builder);
//!
//! runtime.run_to_completion().await?;
//!
//! # Ok(())
//! # }
//! ```
//!

#![forbid(unsafe_code)]

mod actors;
pub mod builders;
pub mod channels;
pub mod converter;
mod errors;
pub mod message_boxes;
mod messages;
#[doc(hidden)]
mod run_actor;
pub mod runtime;
pub mod servers;

pub use actors::*;
pub use builders::*;
pub use channels::*;
pub use converter::*;
pub use errors::*;
pub use message_boxes::*;
pub use messages::*;
pub use runtime::*;
pub use servers::*;

pub use futures;
use futures::channel::mpsc;

#[macro_use]
mod macros;
pub use macros::*;

#[cfg(test)]
#[cfg(feature = "test-helpers")]
pub mod tests;

// FIXME: how to have these examples only available when testing the doc comments?
// #[cfg(test)]
#[doc(hidden)]
pub mod examples;

#[cfg(feature = "test-helpers")]
pub mod test_helpers;
