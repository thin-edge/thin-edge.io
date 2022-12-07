use async_trait::async_trait;
use tedge_actors::{
    new_mailbox, Actor, ChannelError, Mailbox, Recipient, RuntimeError, RuntimeHandle,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MqttConfig {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

/// Open a new MQTT connection using the given config
///
/// A call to `let pub_messages = new_connection(runtime, config, sub_messages)`
/// spawn an MQTT actor and returns a `pub_messages` recipient of `MqttMessage`.
/// * The `sub_messages` recipient argument is where
///   the messages received from the connection will be send to.
/// * The `pub_messages` returned recipient is where
///   the callee will have to send messages to have them published over MQTT.
pub async fn new_connection(
    runtime: &mut RuntimeHandle,
    config: MqttConfig,
    sub_messages: Recipient<MqttMessage>,
) -> Result<Recipient<MqttMessage>, RuntimeError> {
    let (mailbox, address) = new_mailbox(10);

    let actor = MqttActor::new(config);
    runtime.run(actor, mailbox, sub_messages).await?;

    Ok(address.as_recipient())
}

struct MqttActor {
    // Some TCP connection an MQTT server
}

impl MqttActor {
    fn new(_config: MqttConfig) -> Self {
        MqttActor {}
    }

    async fn publish(&self, _message: MqttMessage) {}

    async fn receive(&self) -> Option<MqttMessage> {
        None
    }
}

#[async_trait]
impl Actor for MqttActor {
    type Input = MqttMessage;
    type Output = MqttMessage;
    type Mailbox = Mailbox<MqttMessage>;
    type Peers = Recipient<MqttMessage>;

    async fn run(
        self,
        mut pub_messages: Self::Mailbox,
        mut sub_messages: Self::Peers,
    ) -> Result<(), ChannelError> {
        loop {
            tokio::select! {
                Some(message) = pub_messages.next() => self.publish(message).await,
                Some(message) = self.receive() => sub_messages.send(message).await?,
                else => return Ok(()),
            }
        }
    }
}
