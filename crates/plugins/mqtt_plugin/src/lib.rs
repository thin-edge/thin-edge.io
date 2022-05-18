/// This is a quick implementation an actor around mqtt_channel
///
/// A better implementation would use directly the rumqttc crate to avoid:
/// - duplicated types: a new MqttMessage struct is introduced just to implement the Message trait.
/// - useless queues and tasks: the source and sink objects are moving data around queues.
///   The root issue is that the tedge_actor and mqtt_channel crates use queue in a different manner.
use async_trait::async_trait;
use mqtt_channel::{SinkExt, StreamExt, UnboundedReceiver, UnboundedSender};
use tedge_actors::{Actor, Message, Producer, Reactor, Recipient, RuntimeError};

#[derive(Clone, Debug)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: String,
}

impl Message for MqttMessage {}

pub struct MqttConfig {
    pub port: u16,
    pub subscriptions: Vec<String>,
}

pub struct MqttConnection {
    mqtt_config: mqtt_channel::Config,
}

pub struct MqttMessageSink {
    mqtt_pub: UnboundedSender<mqtt_channel::Message>,
}

pub struct MqttMessageSource {
    mqtt_sub: UnboundedReceiver<mqtt_channel::Message>,
}

#[async_trait]
impl Actor for MqttConnection {
    type Config = MqttConfig;
    type Input = MqttMessage;
    type Output = MqttMessage;
    type Producer = MqttMessageSource;
    type Reactor = MqttMessageSink;

    fn try_new(config: &Self::Config) -> Result<Self, RuntimeError> {
        let subscriptions = config
            .subscriptions
            .clone()
            .try_into()
            .expect("valid topic patterns");
        let mqtt_config = mqtt_channel::Config::default()
            .with_subscriptions(subscriptions)
            .with_port(config.port);
        Ok(MqttConnection { mqtt_config })
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        let connection = mqtt_channel::Connection::new(&self.mqtt_config)
            .await
            .map_err(|_| RuntimeError::ConfigError)?;
        let source = MqttMessageSource {
            mqtt_sub: connection.received,
        };
        let sink = MqttMessageSink {
            mqtt_pub: connection.published,
        };
        Ok((source, sink))
    }
}

#[async_trait]
impl Reactor<MqttMessage, MqttMessage> for MqttMessageSink {
    async fn react(
        &mut self,
        message: MqttMessage,
        _output: &mut impl Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        let topic = mqtt_channel::Topic::new_unchecked(&message.topic);
        let payload = message.payload;
        let raw_message = mqtt_channel::Message::new(&topic, payload);
        Ok(self.mqtt_pub.send(raw_message).await?)
    }
}

#[async_trait]
impl Producer<MqttMessage> for MqttMessageSource {
    async fn produce_messages(
        mut self,
        mut output: impl Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        while let Some(raw_message) = self.mqtt_sub.next().await {
            let message = MqttMessage {
                topic: raw_message.topic.clone().into(),
                payload: raw_message
                    .payload_str()
                    .expect("an utf8 payload")
                    .to_string(),
            };
            output.send_message(message).await?
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
