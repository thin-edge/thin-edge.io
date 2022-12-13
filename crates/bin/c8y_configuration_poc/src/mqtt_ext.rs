use async_trait::async_trait;
use tedge_actors::{
    Actor, ChannelError, DynSender, MessageBoxBuilder, RuntimeError, RuntimeHandle,
    SimpleMessageBox, SimpleMessageBoxBuilder,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

/// Open a new MQTT connection using the given config
///
/// A call to `let pub_message_sender = new_connection(runtime, config, sub_message_sender)`
/// spawn an MQTT actor and returns a `pub_messages` recipient of `MqttMessage`.
/// * The `sub_message_sender` sender argument is used by this actor
///   the forward received messages.
/// * The `pub_message_sender` returned sender is used by the callee
///   to send messages to have them published over MQTT.
pub async fn new_connection(
    runtime: &mut RuntimeHandle,
    config: MqttConfig,
    sub_message_sender: DynSender<MqttMessage>,
) -> Result<DynSender<MqttMessage>, RuntimeError> {
    let mut box_builder = SimpleMessageBoxBuilder::new(10);
    let pub_message_sender = box_builder.get_input();
    box_builder.set_output(sub_message_sender)?;

    let actor = MqttActor::new(config);
    let message_box = box_builder.build()?;
    runtime.run(actor, message_box).await?;

    Ok(pub_message_sender)
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
    type MessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        loop {
            tokio::select! {
                Some(out_message) = messages.next() => self.publish(out_message).await,
                Some(in_message) = self.receive() => messages.send(in_message).await?,
                else => return Ok(()),
            }
        }
    }
}
