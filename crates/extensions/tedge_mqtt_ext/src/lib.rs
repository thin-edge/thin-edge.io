#[cfg(feature = "test-helpers")]
pub mod test_helpers;
#[cfg(test)]
mod tests;

use async_trait::async_trait;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use std::convert::Infallible;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::CombinedReceiver;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tracing::debug;
use tracing::instrument;

pub type MqttConfig = mqtt_channel::Config;
pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::MqttMessage;
pub use mqtt_channel::QoS;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;

pub struct MqttActorBuilder {
    mqtt_config: mqtt_channel::Config,
    input_receiver: CombinedReceiver<MqttMessage>,
    publish_sender: mpsc::Sender<MqttMessage>,
    pub subscriber_addresses: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl MqttActorBuilder {
    pub fn new(config: mqtt_channel::Config) -> Self {
        let (publish_sender, publish_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver = CombinedReceiver::new(publish_receiver, signal_receiver);

        MqttActorBuilder {
            mqtt_config: config,
            input_receiver,
            publish_sender,
            subscriber_addresses: Vec::new(),
            signal_sender,
        }
    }

    pub(crate) fn build_actor(self) -> MqttActor {
        let mut combined_topic_filter = TopicFilter::empty();
        for (topic_filter, _) in self.subscriber_addresses.iter() {
            combined_topic_filter.add_all(topic_filter.to_owned());
        }

        let removed = combined_topic_filter.remove_overlapping_patterns();
        for pattern in combined_topic_filter.patterns() {
            tracing::info!(target: "MQTT sub", "{pattern}");
        }
        for pattern in removed {
            tracing::warn!(target: "MQTT sub", "ignoring overlapping subscription to {pattern}");
        }

        let mqtt_config = self.mqtt_config.with_subscriptions(combined_topic_filter);
        MqttActor::new(mqtt_config, self.input_receiver, self.subscriber_addresses)
    }
}

impl AsMut<MqttConfig> for MqttActorBuilder {
    fn as_mut(&mut self) -> &mut MqttConfig {
        &mut self.mqtt_config
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MqttActorBuilder {
    fn connect_sink(&mut self, subscriptions: TopicFilter, peer: &impl MessageSink<MqttMessage>) {
        let sender = peer.get_sender();
        self.subscriber_addresses.push((subscriptions, sender));
    }
}

impl MessageSink<MqttMessage> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.publish_sender.clone().into()
    }
}

impl RuntimeRequestSink for MqttActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<MqttActor> for MqttActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MqttActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MqttActor {
        self.build_actor()
    }
}

pub struct FromPeers {
    input_receiver: CombinedReceiver<MqttMessage>,
}

pub struct ToPeers {
    peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}

impl FromPeers {
    #[instrument(skip_all, level = "trace")]
    async fn relay_messages_to(
        &mut self,
        outgoing_mqtt: &mut mpsc::UnboundedSender<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        while let Ok(Some(message)) = self.try_recv().await {
            tracing::debug!(target: "MQTT_pub", "{message}");
            SinkExt::send(outgoing_mqtt, message)
                .await
                .map_err(Box::new)?;
        }

        // On shutdown, first close input so no new messages can be pushed
        debug!("closing");
        self.input_receiver.close_input();

        // Then, publish all the messages awaiting to be sent over MQTT
        while let Some(message) = self.recv().await {
            SinkExt::send(outgoing_mqtt, message)
                .await
                .map_err(Box::new)?;
        }
        Ok(())
    }
}

impl ToPeers {
    #[instrument(skip_all, level = "trace")]
    async fn relay_messages_from(
        mut self,
        incoming_mqtt: &mut mpsc::UnboundedReceiver<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        while let Some(message) = incoming_mqtt.next().await {
            tracing::debug!(target: "MQTT_recv", "{message}");
            self.send(message).await?;
        }
        Ok(())
    }

    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        for (topic_filter, peer_sender) in self.peer_senders.iter_mut() {
            if topic_filter.accept(&message) {
                peer_sender.send(message.clone()).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MessageReceiver<MqttMessage> for FromPeers {
    async fn try_recv(&mut self) -> Result<Option<MqttMessage>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.input_receiver.recv().await
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.input_receiver.recv_signal().await
    }
}

pub struct MqttActor {
    mqtt_config: mqtt_channel::Config,
    from_peers: FromPeers,
    to_peers: ToPeers,
}

impl MqttActor {
    fn new(
        mqtt_config: mqtt_channel::Config,
        input_receiver: CombinedReceiver<MqttMessage>,
        peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    ) -> Self {
        MqttActor {
            mqtt_config,
            from_peers: FromPeers { input_receiver },
            to_peers: ToPeers { peer_senders },
        }
    }
}

#[async_trait]
impl Actor for MqttActor {
    fn name(&self) -> &str {
        "MQTT"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut mqtt_client = tokio::select! {
            connection = mqtt_channel::Connection::new(&self.mqtt_config) => {
                connection.map_err(Box::new)?
            }
            Some(RuntimeRequest::Shutdown) = self.from_peers.recv_signal() => {
                // Shutdown requested even before the connection has been established
                return Ok(())
            }
        };

        let sender_task = self
            .from_peers
            .relay_messages_to(&mut mqtt_client.published);
        let receiver_task = self.to_peers.relay_messages_from(&mut mqtt_client.received);

        tedge_utils::futures::select(sender_task, receiver_task).await
    }
}
