use crate::message_boxes::ReceiveMessages;
use crate::ConcurrentServerMessageBox;
use crate::Message;
use crate::MessageBox;
use crate::RuntimeError;
use crate::ServerMessageBox;
use async_trait::async_trait;

/// Enable a struct to be used as an actor.
///
///
#[async_trait]
pub trait Actor: 'static + Sized + Send + Sync {
    /// Type of message box used by this actor
    type MessageBox: MessageBox;

    /// Return the actor instance name
    fn name(&self) -> &str;

    /// Run the actor
    ///
    /// Processing input messages,
    /// updating internal state,
    /// and sending messages to peers.
    async fn run(self, messages: Self::MessageBox) -> Result<(), RuntimeError>;
}

/// An actor that wraps a request-response server
///
/// Requests are processed in turn, leading either to a response or an error.
pub struct ServerActor<S: Server> {
    server: S,
}

impl<S: Server> ServerActor<S> {
    pub fn new(server: S) -> Self {
        ServerActor { server }
    }
}

/// A server defines the behavior of an actor processing requests sending back responses.
///
/// A `Server` is defined by:
/// - a `Request` message type for the requests received from client actors,
/// - a `Response` message type for the responses sent back to the requesters,
/// - an asynchronous `handle` method that defines how the server responds to a request,
///   updating its state and possibly performing side effects.
///
/// ```
/// # use crate::tedge_actors::{Server, MessageBox, SimpleMessageBox};
/// # use async_trait::async_trait;
///
/// # use crate::tedge_actors::examples;
/// # type Operation = examples::calculator::Operation;
/// # type Update = examples::calculator::Update;
///
/// /// State of the calculator server
/// #[derive(Default)]
/// struct Calculator {
///     state: i64,
/// }
///
/// /// Implementation of the calculator behavior
/// #[async_trait]
/// impl Server for Calculator {
///
///     type Request = Operation;
///     type Response = Update;
///
///     fn name(&self) -> &str {
///         "Calculator"
///     }
///
///     async fn handle(&mut self, request: Self::Request) -> Self::Response {
///         // Act accordingly to the request
///         let from = self.state;
///         let to = match request {
///            Operation::Add(x) => from + x,
///            Operation::Multiply(x) => from * x,
///         };
///
///         // Update the server state
///         self.state = to;
///
///         // Return the response
///         Update{from,to}
///     }
/// }
/// ```
///
/// To be used as an actor, a `Server` is wrapped into a [ServerActor](crate::ServerActor)
///
/// ```
/// # use tedge_actors::{Actor, Builder, MessageBox, NoConfig, ReceiveMessages, ServerActor, SimpleMessageBox, SimpleMessageBoxBuilder};
/// use tedge_actors::test_helpers::ServiceProviderExt;
/// # use crate::tedge_actors::examples::calculator::*;
/// #
/// # #[tokio::main]
/// # async fn main_test() {
/// #
/// // Create an actor to handle the requests to a server
/// let server = Calculator::default();
/// let actor = ServerActor::new(server);
///
/// // As for any actor, one needs a bidirectional channel to the message box of the server.
/// let mut actor_box_builder = SimpleMessageBoxBuilder::new("Actor", 10);
/// let mut client_box = actor_box_builder.new_client_box(NoConfig);
/// let server_box = actor_box_builder.build();
///
/// // The actor is then spawn in the background with its message box.
/// tokio::spawn(actor.run(server_box));
///
/// // One can then interact with the actor
/// // Note that now each request is prefixed by a number: the id of the requester
/// client_box.send((1,Operation::Add(4))).await.expect("message sent");
/// client_box.send((2,Operation::Multiply(10))).await.expect("message sent");
/// client_box.send((1,Operation::Add(2))).await.expect("message sent");
///
/// // Observing the server behavior,
/// // note that the responses come back associated to the id of the requester.
/// assert_eq!(client_box.recv().await, Some((1, Update{from:0,to:4})));
/// assert_eq!(client_box.recv().await, Some((2, Update{from:4,to:40})));
/// assert_eq!(client_box.recv().await, Some((1, Update{from:40,to:42})));
///
/// # }
/// ```
///

#[async_trait]
pub trait Server: 'static + Sized + Send + Sync {
    type Request: Message;
    type Response: Message;

    /// Return the server name
    fn name(&self) -> &str;

    /// Handle the request returning the response when done
    ///
    /// For such a server to return errors, the response type must be a `Result`.
    async fn handle(&mut self, request: Self::Request) -> Self::Response;
}

#[async_trait]
impl<S: Server> Actor for ServerActor<S> {
    type MessageBox = ServerMessageBox<S::Request, S::Response>;

    fn name(&self) -> &str {
        self.server.name()
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        let mut server = self.server;
        while let Some((client_id, request)) = messages.recv().await {
            let result = server.handle(request).await;
            messages.send((client_id, result)).await?
        }
        Ok(())
    }
}

/// An actor that wraps a request-response protocol
///
/// Requests are processed concurrently (up to some max concurrency level).
///
/// The server must be `Clone` to create a fresh server handle for each request.
pub struct ConcurrentServerActor<S: Server + Clone> {
    server: S,
}

impl<S: Server + Clone> ConcurrentServerActor<S> {
    pub fn new(server: S) -> Self {
        ConcurrentServerActor { server }
    }
}

#[async_trait]
impl<S: Server + Clone> Actor for ConcurrentServerActor<S> {
    type MessageBox = ConcurrentServerMessageBox<S::Request, S::Response>;

    fn name(&self) -> &str {
        self.server.name()
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
        while let Some((client_id, request)) = messages.recv().await {
            // Spawn the request
            let mut server = self.server.clone();
            let pending_result = tokio::spawn(async move {
                let result = server.handle(request).await;
                (client_id, result)
            });

            // Send the response back to the client
            messages.send_response_once_done(pending_result)
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use crate::test_helpers::ServiceProviderExt;
    use crate::*;
    use async_trait::async_trait;
    use futures::channel::mpsc;
    use futures::StreamExt;
    use tokio::spawn;

    struct Echo;

    #[async_trait]
    impl Actor for Echo {
        type MessageBox = SimpleMessageBox<String, String>;

        fn name(&self) -> &str {
            "Echo"
        }

        async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
            while let Some(message) = messages.recv().await {
                messages.send(message).await?
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn running_an_actor_without_a_runtime() {
        let actor = Echo;
        let mut box_builder = SimpleMessageBoxBuilder::new("test", 16);
        let mut client_message_box = box_builder.new_client_box(NoConfig);
        let actor_message_box = box_builder.build();
        let actor_task = spawn(actor.run(actor_message_box));

        // Messages sent to the actor
        assert!(client_message_box.send("Hello".to_string()).await.is_ok());
        assert!(client_message_box.send("World".to_string()).await.is_ok());

        // Messages received from the actor
        assert_eq!(client_message_box.recv().await, Some("Hello".to_string()));
        assert_eq!(client_message_box.recv().await, Some("World".to_string()));

        // When there is no more input message senders
        client_message_box.close_output();

        // The actor stops
        actor_task
            .await
            .expect("the actor run to completion")
            .expect("the actor returned Ok");

        // And the clients receives an end of stream event
        assert_eq!(client_message_box.recv().await, None);
    }

    #[tokio::test]
    async fn an_actor_can_send_messages_to_specific_peers() {
        let (output_sender, mut output_receiver) = mpsc::channel(10);

        let actor = ActorWithSpecificMessageBox;
        let (input_sender, message_box) =
            SpecificMessageBox::new_box(actor.name(), 10, output_sender.into());
        let actor_task = spawn(actor.run(message_box));

        spawn(async move {
            let mut sender: DynSender<&str> = adapt(&input_sender);
            sender.send("Do this").await.expect("sent");
            sender.send("Do nothing").await.expect("sent");
            sender.send("Do that and this").await.expect("sent");
            sender.send("Do that").await.expect("sent");
        });

        actor_task
            .await
            .expect("the actor run to completion")
            .expect("the actor returned Ok");

        assert_eq!(
            output_receiver.next().await,
            Some(DoMsg::DoThis(DoThis("Do this".into())))
        );
        assert_eq!(
            output_receiver.next().await,
            Some(DoMsg::DoThis(DoThis("Do that and this".into())))
        );
        assert_eq!(
            output_receiver.next().await,
            Some(DoMsg::DoThat(DoThat("Do that and this".into())))
        );
        assert_eq!(
            output_receiver.next().await,
            Some(DoMsg::DoThat(DoThat("Do that".into())))
        );
    }

    pub struct ActorWithSpecificMessageBox;

    #[async_trait]
    impl Actor for ActorWithSpecificMessageBox {
        type MessageBox = SpecificMessageBox;

        fn name(&self) -> &str {
            "ActorWithSpecificMessageBox"
        }

        async fn run(self, mut messages: Self::MessageBox) -> Result<(), RuntimeError> {
            while let Some(message) = messages.next().await {
                if message.contains("this") {
                    messages.do_this(message.to_string()).await?
                }
                if message.contains("that") {
                    messages.do_that(message.to_string()).await?
                }
            }
            Ok(())
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DoThis(String);

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DoThat(String);

    fan_in_message_type!(DoMsg[DoThis,DoThat] : Clone , Debug , Eq , PartialEq);

    pub struct SpecificMessageBox {
        name: String,
        input: mpsc::Receiver<String>,
        peer_1: DynSender<DoThis>,
        peer_2: DynSender<DoThat>,
    }

    impl SpecificMessageBox {
        fn new_box(
            name: &str,
            capacity: usize,
            output: DynSender<DoMsg>,
        ) -> (DynSender<String>, Self) {
            let (sender, input) = mpsc::channel(capacity);
            let peer_1 = adapt(&output);
            let peer_2 = adapt(&output);
            let message_box = SpecificMessageBox {
                name: name.to_string(),
                input,
                peer_1,
                peer_2,
            };
            (sender.into(), message_box)
        }

        pub async fn next(&mut self) -> Option<String> {
            self.input.next().await
        }

        pub async fn do_this(&mut self, action: String) -> Result<(), ChannelError> {
            self.peer_1.send(DoThis(action)).await
        }

        pub async fn do_that(&mut self, action: String) -> Result<(), ChannelError> {
            self.peer_2.send(DoThat(action)).await
        }
    }

    #[async_trait]
    impl MessageBox for SpecificMessageBox {
        type Input = String;
        type Output = DoMsg;

        fn name(&self) -> &str {
            &self.name
        }
    }
}
