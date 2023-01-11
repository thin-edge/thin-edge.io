use crate::ChannelError;
use crate::ConcurrentServiceMessageBox;
use crate::Message;
use crate::MessageBox;
use crate::ServiceMessageBox;
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
    async fn run(self, messages: Self::MessageBox) -> Result<(), ChannelError>;
}

/// An actor that wraps a request-response service
///
/// Requests are processed in turn, leading either to a response or an error.
pub struct ServiceActor<S: Service> {
    service: S,
}

impl<S: Service> ServiceActor<S> {
    pub fn new(service: S) -> Self {
        ServiceActor { service }
    }
}

#[async_trait]
pub trait Service: 'static + Sized + Send + Sync {
    type Request: Message;
    type Response: Message;

    /// Return the service name
    fn name(&self) -> &str;

    /// Handle the request returning the response when done
    ///
    /// For such a service to return errors, the response type must be a `Result`.
    async fn handle(&mut self, request: Self::Request) -> Self::Response;
}

#[async_trait]
impl<S: Service> Actor for ServiceActor<S> {
    type MessageBox = ServiceMessageBox<S::Request, S::Response>;

    fn name(&self) -> &str {
        self.service.name()
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        let mut service = self.service;
        while let Some((client_id, request)) = messages.recv().await {
            let result = service.handle(request).await;
            messages.send((client_id, result)).await?
        }
        Ok(())
    }
}

/// An actor that wraps a request-response service
///
/// Requests are processed concurrently (up to some max concurrency level).
///
/// The service must be `Clone` to create a fresh service handle for each request.
pub struct ConcurrentServiceActor<S: Service + Clone> {
    service: S,
}

impl<S: Service + Clone> ConcurrentServiceActor<S> {
    pub fn new(service: S) -> Self {
        ConcurrentServiceActor { service }
    }
}

#[async_trait]
impl<S: Service + Clone> Actor for ConcurrentServiceActor<S> {
    type MessageBox = ConcurrentServiceMessageBox<S::Request, S::Response>;

    fn name(&self) -> &str {
        self.service.name()
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some((client_id, request)) = messages.recv().await {
            // Spawn the request
            let mut service = self.service.clone();
            let pending_result = tokio::spawn(async move {
                let result = service.handle(request).await;
                (client_id, result)
            });

            // Send the response back to the client
            messages.send_response_once_done(pending_result)
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_utils::VecRecipient;
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

        async fn run(
            mut self,
            mut messages: SimpleMessageBox<String, String>,
        ) -> Result<(), ChannelError> {
            while let Some(message) = messages.recv().await {
                messages.send(message).await?
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn running_an_actor_without_a_runtime() {
        let actor = Echo;
        let (mut client_message_box, actor_message_box) = SimpleMessageBox::new_channel("test");

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
        let output_messages: VecRecipient<DoMsg> = VecRecipient::default();

        let actor = ActorWithSpecificMessageBox;
        let (actor_input, message_box) =
            SpecificMessageBox::new_box(actor.name(), 10, output_messages.as_sender());
        let actor_task = spawn(actor.run(message_box));

        spawn(async move {
            let mut sender: DynSender<&str> = adapt(&actor_input.into());
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
            output_messages.collect().await,
            vec![
                DoMsg::DoThis(DoThis("Do this".into())),
                DoMsg::DoThis(DoThis("Do that and this".into())),
                DoMsg::DoThat(DoThat("Do that and this".into())),
                DoMsg::DoThat(DoThat("Do that".into())),
            ]
        )
    }

    pub struct ActorWithSpecificMessageBox;

    #[async_trait]
    impl Actor for ActorWithSpecificMessageBox {
        type MessageBox = SpecificMessageBox;

        fn name(&self) -> &str {
            "ActorWithSpecificMessageBox"
        }

        async fn run(self, mut messages: SpecificMessageBox) -> Result<(), ChannelError> {
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

        async fn recv(&mut self) -> Option<Self::Input> {
            self.input.next().await
        }

        async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
            match message {
                DoMsg::DoThis(message) => self.peer_1.send(message).await,
                DoMsg::DoThat(message) => self.peer_2.send(message).await,
            }
        }

        fn turn_logging_on(&mut self, _on: bool) {}

        fn name(&self) -> &str {
            &self.name
        }

        fn logging_is_on(&self) -> bool {
            false
        }
    }
}
