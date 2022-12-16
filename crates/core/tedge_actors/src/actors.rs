use crate::{ChannelError, MessageBox};
use async_trait::async_trait;

/// Enable a struct to be used as an actor.
///
///
#[async_trait]
pub trait Actor: 'static + Sized + Send + Sync {
    /// Type of message box used by this actor
    type MessageBox: MessageBox;

    /// Run the actor
    ///
    /// Processing input messages,
    /// updating internal state,
    /// and sending messages to peers.
    async fn run(self, messages: Self::MessageBox) -> Result<(), ChannelError>;
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
        let actor_output_collector: VecRecipient<String> = VecRecipient::default();

        let (actor_input_sender, message_box) =
            SimpleMessageBox::new_box(10, actor_output_collector.as_sender());

        let actor = Echo;
        let actor_task = spawn(actor.run(message_box));

        spawn(async move {
            let mut sender: DynSender<&str> = adapt(&actor_input_sender.into());
            sender
                .send("Hello")
                .await
                .expect("the actor is still running");
            sender
                .send("World")
                .await
                .expect("the actor is still running");
        });

        actor_task
            .await
            .expect("the actor run to completion")
            .expect("the actor returned Ok");

        assert_eq!(
            actor_output_collector.collect().await,
            vec!["Hello".to_string(), "World".to_string()]
        )
    }

    #[tokio::test]
    async fn an_actor_can_send_messages_to_specific_peers() {
        let output_messages: VecRecipient<DoMsg> = VecRecipient::default();

        let actor = ActorWithSpecificMessageBox;
        let (actor_input, message_box) =
            SpecificMessageBox::new_box(10, output_messages.as_sender());
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
        input: mpsc::Receiver<String>,
        peer_1: DynSender<DoThis>,
        peer_2: DynSender<DoThat>,
    }

    impl SpecificMessageBox {
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

        fn new_box(
            capacity: usize,
            output: DynSender<Self::Output>,
        ) -> (DynSender<Self::Input>, Self) {
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
    }
}
