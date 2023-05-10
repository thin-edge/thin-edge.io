use crate::RuntimeError;
use async_trait::async_trait;

/// Enable a struct to be used as an actor: a processing unit that interact using asynchronous messages.
///
/// This trait provides an actor the flexibility to:
///
/// - own and freely update some internal state,
/// - use a specific [message box](crate::message_boxes) to address specific communication needs:
///   - pub/sub,
///   - request/response,
///   - message priority,
///   - concurrent message processing, ...
/// - freely interleave message reception and emission in its [Actor::run()](crate::Actor::run) event loop:
///   - reacting to peer messages as well as internal events,
///   - sending responses for requests, possibly deferring some responses,
///   - acting as a source of messages ...
#[async_trait]
pub trait Actor: 'static + Send + Sync {
    /// Return the actor instance name
    fn name(&self) -> &str;

    /// Run the actor
    ///
    /// Processing input messages,
    /// updating internal state,
    /// and sending messages to peers.
    async fn run(&mut self) -> Result<(), RuntimeError>;
}

#[cfg(test)]
#[cfg(feature = "test-helpers")]
pub mod tests {
    use crate::test_helpers::ServiceProviderExt;
    use crate::*;
    use async_trait::async_trait;
    use futures::channel::mpsc;
    use futures::StreamExt;
    use tokio::spawn;

    struct Echo {
        messages: SimpleMessageBox<String, String>,
    }

    #[async_trait]
    impl Actor for Echo {
        fn name(&self) -> &str {
            "Echo"
        }

        async fn run(&mut self) -> Result<(), RuntimeError> {
            while let Some(message) = self.messages.recv().await {
                self.messages.send(message).await?
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn running_an_actor_without_a_runtime() {
        let mut box_builder = SimpleMessageBoxBuilder::new("test", 16);
        let mut client_message_box = box_builder.new_client_box(NoConfig);
        let actor_message_box = box_builder.build();
        let mut actor = Echo {
            messages: actor_message_box,
        };
        let actor_task = spawn(async move { actor.run().await });

        // Messages sent to the actor
        assert!(client_message_box.send("Hello".to_string()).await.is_ok());
        assert!(client_message_box.send("World".to_string()).await.is_ok());

        // Messages received from the actor
        assert_eq!(client_message_box.recv().await, Some("Hello".to_string()));
        assert_eq!(client_message_box.recv().await, Some("World".to_string()));

        // When there is no more input message senders
        client_message_box.close_sender();

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

        let (input_sender, message_box) = SpecificMessageBox::new_box(10, output_sender.into());
        let mut actor = ActorWithSpecificMessageBox {
            messages: message_box,
        };
        let actor_task = spawn(async move { actor.run().await });

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

    pub struct ActorWithSpecificMessageBox {
        messages: SpecificMessageBox,
    }

    #[async_trait]
    impl Actor for ActorWithSpecificMessageBox {
        fn name(&self) -> &str {
            "ActorWithSpecificMessageBox"
        }

        async fn run(&mut self) -> Result<(), RuntimeError> {
            while let Some(message) = self.messages.next().await {
                if message.contains("this") {
                    self.messages.do_this(message.to_string()).await?
                }
                if message.contains("that") {
                    self.messages.do_that(message.to_string()).await?
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
        input: mpsc::Receiver<String>,
        peer_1: DynSender<DoThis>,
        peer_2: DynSender<DoThat>,
    }

    impl SpecificMessageBox {
        fn new_box(capacity: usize, output: DynSender<DoMsg>) -> (DynSender<String>, Self) {
            let (sender, input) = mpsc::channel(capacity);
            let peer_1 = adapt(&output);
            let peer_2 = adapt(&output);
            let message_box = SpecificMessageBox {
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
}
