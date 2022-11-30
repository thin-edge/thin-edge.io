use crate::ChannelError;
use crate::{Mailbox, Message, Recipient};
use async_trait::async_trait;

/// Enable a struct to be used as an actor.
///
///
#[async_trait]
pub trait Actor: 'static + Sized + Send + Sync {
    /// Type of input messages this actor consumes
    type Input: Message;

    /// Type of output messages this actor produces
    type Output: Message;

    /// Type of the mailbox used by this actor
    type Mailbox: From<Mailbox<Self::Input>> + Send + Sync;

    /// Type of the peers that actor is connected to
    type Peers: From<Recipient<Self::Output>> + Send + Sync;

    /// Run the actor
    ///
    /// Processing input messages,
    /// updating internal state,
    /// and sending messages to peers.
    async fn run(self, messages: Self::Mailbox, peers: Self::Peers) -> Result<(), ChannelError>;
}

#[cfg(test)]
mod tests {
    use crate::test_utils::VecRecipient;
    use crate::*;
    use async_trait::async_trait;
    use tokio::spawn;

    struct Echo;

    #[async_trait]
    impl Actor for Echo {
        type Input = String;
        type Output = String;
        type Mailbox = Mailbox<String>;
        type Peers = Recipient<String>;

        async fn run(
            mut self,
            mut messages: Mailbox<Self::Input>,
            mut peers: Self::Peers,
        ) -> Result<(), ChannelError> {
            while let Some(message) = messages.next().await {
                peers.send(message).await?
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn running_an_actor_without_a_runtime() {
        let messages: VecRecipient<String> = VecRecipient::default();

        let actor = Echo;
        let (mailbox, input) = new_mailbox(10);
        let output = messages.as_recipient();

        let actor_task = spawn(actor.run(mailbox, output));

        spawn(async move {
            let mut input = input.as_recipient();
            input
                .send("Hello")
                .await
                .expect("the actor is still running");
            input
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

        let actor = ActorWithSpecificPeers;
        let (mailbox, input) = new_mailbox(10);
        let output = output_messages.as_recipient();
        let actor_task = spawn(actor.run(mailbox, output.into()));

        spawn(async move {
            let mut input = input.as_recipient();
            input.send("Do this").await.expect("sent");
            input.send("Do nothing").await.expect("sent");
            input.send("Do that and this").await.expect("sent");
            input.send("Do that").await.expect("sent");
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

    pub struct ActorWithSpecificPeers;

    #[async_trait]
    impl Actor for ActorWithSpecificPeers {
        type Input = String;
        type Output = DoMsg;
        type Mailbox = Mailbox<String>;
        type Peers = SpecificPeers;

        async fn run(
            self,
            mut messages: Mailbox<String>,
            mut peers: Self::Peers,
        ) -> Result<(), ChannelError> {
            while let Some(message) = messages.next().await {
                if message.contains("this") {
                    peers.do_this(message.to_string()).await?
                }
                if message.contains("that") {
                    peers.do_that(message.to_string()).await?
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

    pub struct SpecificPeers {
        pub peer_1: Recipient<DoThis>,
        pub peer_2: Recipient<DoThat>,
    }

    impl SpecificPeers {
        pub async fn do_this(&mut self, action: String) -> Result<(), ChannelError> {
            self.peer_1.send(DoThis(action)).await
        }

        pub async fn do_that(&mut self, action: String) -> Result<(), ChannelError> {
            self.peer_2.send(DoThat(action)).await
        }
    }

    impl From<Recipient<DoMsg>> for SpecificPeers {
        fn from(recipient: Recipient<DoMsg>) -> Self {
            SpecificPeers {
                peer_1: adapt(&recipient),
                peer_2: adapt(&recipient),
            }
        }
    }
}
