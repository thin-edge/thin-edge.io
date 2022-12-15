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
        let messages: VecRecipient<String> = VecRecipient::default();

        let mut message_box_builder = SimpleMessageBoxBuilder::new(10);
        message_box_builder
            .set_output(messages.as_sender())
            .expect("A builder with output set");

        let actor = Echo;
        let actor_sender = adapt(&message_box_builder.get_input());
        let actor_messages = message_box_builder.build().expect("A message box");
        let actor_task = spawn(actor.run(actor_messages));

        spawn(async move {
            let mut sender: DynSender<&str> = actor_sender.into();
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
            messages.collect().await,
            vec!["Hello".to_string(), "World".to_string()]
        )
    }

    #[tokio::test]
    async fn an_actor_can_send_messages_to_specific_peers() {
        let output_messages: VecRecipient<DoMsg> = VecRecipient::default();

        let mut message_box_builder = SpecificMessageBoxBuilder::new(10);
        message_box_builder
            .set_output(output_messages.as_sender())
            .expect("a builder ready to use");

        let actor = ActorWithSpecificMessageBox;
        let actor_sender = adapt(&message_box_builder.get_input());
        let actor_messages = message_box_builder.build().expect("a message box");
        let actor_task = spawn(actor.run(actor_messages));

        spawn(async move {
            let mut sender: DynSender<&str> = actor_sender.into();
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
    }

    pub struct SpecificMessageBoxBuilder {
        sender: mpsc::Sender<String>,
        input: mpsc::Receiver<String>,
        peer_1: Option<DynSender<DoThis>>,
        peer_2: Option<DynSender<DoThat>>,
    }

    impl SpecificMessageBoxBuilder {
        pub fn new(size: usize) -> Self {
            let (sender, input) = mpsc::channel(size);
            SpecificMessageBoxBuilder {
                sender,
                input,
                peer_1: None,
                peer_2: None,
            }
        }

        pub fn build(self) -> Result<SpecificMessageBox, LinkError> {
            Ok(SpecificMessageBox {
                input: self.input,
                peer_1: self.peer_1.expect("peer_1 has been set"),
                peer_2: self.peer_2.expect("peer_2 has been set"),
            })
        }

        pub fn get_input(&self) -> DynSender<String> {
            self.sender.clone().into()
        }

        pub fn set_output(&mut self, output: DynSender<DoMsg>) -> Result<(), LinkError> {
            self.peer_1 = Some(adapt(&output));
            self.peer_2 = Some(adapt(&output));
            Ok(())
        }
    }
}
