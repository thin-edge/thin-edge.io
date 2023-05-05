//! A server defines the behavior of an actor processing requests, sending back responses to the callers.
//!
//! A `Server` is defined by:
//! - a `Request` message type for the requests received from client actors,
//! - a `Response` message type for the responses sent back to the requesters,
//! - an asynchronous `handle` method that defines how the server responds to a request,
//!   updating its state and possibly performing side effects.
//!
//! ```
//! # use crate::tedge_actors::{Server, SimpleMessageBox};
//! # use async_trait::async_trait;
//!
//! # use crate::tedge_actors::examples;
//! # type Operation = examples::calculator::Operation;
//! # type Update = examples::calculator::Update;
//! /// State of the calculator server
//! #[derive(Default)]
//! struct Calculator {
//!     state: i64,
//! }
//!
//! /// Implementation of the calculator behavior
//! #[async_trait]
//! impl Server for Calculator {
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
//!         // Update the server state
//!         self.state = to;
//!
//!         // Return the response
//!         Update{from,to}
//!     }
//! }
//! ```
//!
//! To be used as an actor, a `Server` is wrapped into a [ServerActor](crate::ServerActor)
//!
//! ```
//! # use tedge_actors::{Actor, Builder, NoConfig, MessageReceiver, Sender, ServerActor, SimpleMessageBox, SimpleMessageBoxBuilder};
//! # use crate::tedge_actors::examples::calculator_server::*;
//! #
//! #[cfg(feature = "test-helpers")]
//! # #[tokio::main]
//! # async fn main_test() {
//! # use tedge_actors::test_helpers::ServiceProviderExt;
//! #
//! // As for any actor, one needs a bidirectional channel to the message box of the server.
//! let mut actor_box_builder = SimpleMessageBoxBuilder::new("Actor", 10);
//! let mut client_box = actor_box_builder.new_client_box(NoConfig);
//! let server_box = actor_box_builder.build();
//!
//! // Create an actor to handle the requests to a server
//! let server = Calculator::default();
//! let mut actor = ServerActor::new(server, server_box);
//!
//! // The actor is then spawn in the background with its message box.
//! tokio::spawn(async move { actor.run().await } );
//!
//! // One can then interact with the actor
//! // Note that now each request is prefixed by a number: the id of the requester
//! client_box.send((1,Operation::Add(4))).await.expect("message sent");
//! client_box.send((2,Operation::Multiply(10))).await.expect("message sent");
//! client_box.send((1,Operation::Add(2))).await.expect("message sent");
//!
//! // Observing the server behavior,
//! // note that the responses come back associated to the id of the requester.
//! assert_eq!(client_box.recv().await, Some((1, Update{from:0,to:4})));
//! assert_eq!(client_box.recv().await, Some((2, Update{from:4,to:40})));
//! assert_eq!(client_box.recv().await, Some((1, Update{from:40,to:42})));
//!
//! # }
//! ```
//!
mod actors;
mod builders;
mod keyed_messages;
mod message_boxes;

pub use actors::*;
pub use builders::*;
pub use keyed_messages::*;
pub use message_boxes::*;

use crate::Message;
use async_trait::async_trait;

/// Define how a server process a request
#[async_trait]
pub trait Server: 'static + Sized + Send + Sync {
    /// The type of the requests send by clients
    type Request: Message;

    /// The type of the responses returned to clients
    type Response: Message;

    /// Return the server name
    fn name(&self) -> &str;

    /// Handle the request returning the response when done
    ///
    /// For such a server to return errors, the response type must be a `Result`.
    async fn handle(&mut self, request: Self::Request) -> Self::Response;
}
