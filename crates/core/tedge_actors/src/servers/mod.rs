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
//! # use tedge_actors::{Actor, Builder, NoConfig, MessageReceiver, Sender, ServerActorBuilder, ServerConfig, Sequential};
//! # use crate::tedge_actors::examples::calculator_server::*;
//! #
//! #[cfg(feature = "test-helpers")]
//! # #[tokio::main]
//! # async fn main_test() {
//! # use tedge_actors::test_helpers::ServiceProviderExt;
//! #
//! // As for any actor, one needs a handle to the message box of the server.
//! // The simpler is to use a builder.
//! let config = ServerConfig {
//!             capacity: 16,
//!             max_concurrency: 4,
//! };
//! let actor = ServerActorBuilder::new(Calculator::default(), &config, Sequential);
//! let mut handle = actor.request_sender();
//!
//! // This handle can then be used to connect client message boxes
//! let mut client_1 = handle.new_client_box();
//! let mut client_2 = handle.new_client_box();
//!
//! // The actor is then spawn in the background.
//! tokio::spawn(async move { actor.run().await } );
//!
//! // One can then interact with the actor
//! client_1.send(Operation::Add(4)).await.expect("message sent");
//! client_2.send(Operation::Multiply(10)).await.expect("message sent");
//! client_1.send(Operation::Add(2)).await.expect("message sent");
//!
//! // Observing the server behavior,
//! // each client receiving the responses to its requests
//! // which are processed in turn.
//! assert_eq!(client_1.recv().await, Some(Update{from:0,to:4}));
//! assert_eq!(client_2.recv().await, Some(Update{from:4,to:40}));
//! assert_eq!(client_1.recv().await, Some(Update{from:40,to:42}));
//!
//! # }
//! ```
//!
mod actors;
mod builders;
mod message_boxes;

pub use actors::*;
pub use builders::*;
pub use message_boxes::*;
use std::fmt::Debug;

use crate::DynSender;
use crate::Message;
use crate::MessageSink;
use crate::MessageSource;
use crate::NoConfig;
use crate::Sender;
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

/// Wrap a request with a [Sender] to send the response to
///
/// Requests are sent to server actors using such envelopes telling where to send the responses.
pub struct RequestEnvelope<Request, Response> {
    pub request: Request,
    pub reply_to: Box<dyn Sender<Response>>,
}

impl<Request: Debug, Response> Debug for RequestEnvelope<Request, Response> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.request.fmt(f)
    }
}

impl<Request, Response> AsRef<Request> for RequestEnvelope<Request, Response> {
    fn as_ref(&self) -> &Request {
        &self.request
    }
}

/// A request sender to some [Server]
pub type DynRequestSender<Request, Response> = DynSender<RequestEnvelope<Request, Response>>;

/// A connector to a [Server] expecting Request and returning Response.
pub trait Service<Request: Message, Response: Message>:
    MessageSink<RequestEnvelope<Request, Response>, NoConfig>
{
    /// Connect a request message box to the server box under construction
    fn add_requester(&mut self, response_sender: DynSender<Response>) -> DynSender<Request>;

    fn add_client(
        &mut self,
        client: &mut (impl MessageSource<Request, NoConfig> + MessageSink<Response, NoConfig>),
    );
}

impl<T, Request: Message, Response: Message> Service<Request, Response> for T
where
    T: MessageSink<RequestEnvelope<Request, Response>, NoConfig>,
{
    fn add_requester(&mut self, reply_to: DynSender<Response>) -> DynSender<Request> {
        let request_sender = RequestSender {
            sender: self.get_sender(),
            reply_to,
        };
        request_sender.into()
    }

    fn add_client(
        &mut self,
        client: &mut (impl MessageSource<Request, NoConfig> + MessageSink<Response, NoConfig>),
    ) {
        let request_sender = self.add_requester(client.get_sender());
        client.register_peer(NoConfig, request_sender);
    }
}
