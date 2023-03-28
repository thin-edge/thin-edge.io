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
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;
use tedge_actors::WrappedInput;

pub type MqttConfig = mqtt_channel::Config;
pub type MqttMessage = mqtt_channel::Message;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::Topic;
pub use mqtt_channel::TopicFilter;

pub struct MqttActorBuilder {
    pub mqtt_config: mqtt_channel::Config,
    input_receiver: LoggingReceiver<MqttMessage>,
    publish_sender: mpsc::Sender<MqttMessage>,
    pub subscriber_addresses: Vec<(TopicFilter, LoggingSender<MqttMessage>)>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl MqttActorBuilder {
    pub fn new(config: mqtt_channel::Config) -> Self {
        let (publish_sender, publish_receiver) = mpsc::channel(10);
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        let input_receiver = LoggingReceiver::new("MQTT".into(), publish_receiver, signal_receiver);

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
        let mqtt_config = self.mqtt_config.with_subscriptions(combined_topic_filter);
        let mqtt_message_box = MqttMessageBox::new(self.input_receiver, self.subscriber_addresses);

        MqttActor::new(mqtt_config, mqtt_message_box)
    }
}

impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> for MqttActorBuilder {
    fn add_peer(&mut self, peer: &mut impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter>) {
        let subscriptions = peer.get_config();
        let sender = LoggingSender::new("MQTT".into(), peer.get_response_sender());
        self.subscriber_addresses.push((subscriptions, sender));
        peer.set_request_sender(self.publish_sender.clone().into())
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MqttActorBuilder {
    fn register_peer(&mut self, subscriptions: TopicFilter, sender: DynSender<MqttMessage>) {
        let sender = LoggingSender::new("MQTT".into(), sender);
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

pub struct MqttMessageBox {
    input_receiver: LoggingReceiver<MqttMessage>,
    peer_senders: Vec<(TopicFilter, LoggingSender<MqttMessage>)>,
}

impl MqttMessageBox {
    fn new(
        input_receiver: LoggingReceiver<MqttMessage>,
        peer_senders: Vec<(TopicFilter, LoggingSender<MqttMessage>)>,
    ) -> Self {
        MqttMessageBox {
            input_receiver,
            peer_senders,
        }
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
impl MessageReceiver<MqttMessage> for MqttMessageBox {
    async fn try_recv(&mut self) -> Result<Option<MqttMessage>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv_message(&mut self) -> Option<WrappedInput<MqttMessage>> {
        self.input_receiver.recv_message().await
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.input_receiver.recv().await
    }
}

pub struct MqttActor {
    mqtt_config: mqtt_channel::Config,
    messages: MqttMessageBox,
}

impl MqttActor {
    fn new(mqtt_config: mqtt_channel::Config, messages: MqttMessageBox) -> Self {
        MqttActor {
            mqtt_config,
            messages,
        }
    }
}

#[async_trait]
impl Actor for MqttActor {
    fn name(&self) -> &str {
        "MQTT"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let mut mqtt_client = mqtt_channel::Connection::new(&self.mqtt_config)
            .await
            .map_err(Box::new)?;

        loop {
            tokio::select! {
                message_or_signal = self.messages.try_recv() => {
                    match message_or_signal {
                        Ok(Some(message)) => {
                                                mqtt_client
                            .published
                            .send(message)
                            .await
                            .map_err(Box::new)?
                        }
                        Ok(None) | Err(RuntimeRequest::Shutdown) => break,
                    }
                }
                Some(message) = mqtt_client.received.next() => {
                    self.messages.send(message).await?
                },
                else => break,
            }
        }
        Ok(())
    }
}
