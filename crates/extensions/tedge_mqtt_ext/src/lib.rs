#[cfg(test)]
mod tests;

use async_trait::async_trait;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use std::convert::Infallible;
use tedge_actors::futures::channel::mpsc::channel;
use tedge_actors::futures::channel::mpsc::Receiver;
use tedge_actors::futures::channel::mpsc::Sender;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;

pub type MqttConfig = mqtt_channel::Config;
pub type MqttMessage = mqtt_channel::Message;

pub struct MqttActorBuilder {
    pub mqtt_config: mqtt_channel::Config,
    pub publish_channel: (Sender<MqttMessage>, Receiver<MqttMessage>),
    pub subscriber_addresses: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}

impl MqttActorBuilder {
    pub fn new(config: mqtt_channel::Config) -> Self {
        MqttActorBuilder {
            mqtt_config: config,
            publish_channel: channel(10),
            subscriber_addresses: Vec::new(),
        }
    }

    pub(crate) fn build_actor_and_box(self) -> (MqttActor, MqttMessageBox) {
        let mut combined_topic_filter = TopicFilter::empty();
        for (topic_filter, _) in self.subscriber_addresses.iter() {
            combined_topic_filter.add_all(topic_filter.to_owned());
        }
        let mqtt_config = self.mqtt_config.with_subscriptions(combined_topic_filter);
        let mqtt_message_box =
            MqttMessageBox::new(self.publish_channel.1, self.subscriber_addresses);

        let mqtt_actor = MqttActor::new(mqtt_config);
        (mqtt_actor, mqtt_message_box)
    }
}

impl MessageBoxSocket<MqttMessage, MqttMessage, TopicFilter> for MqttActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<MqttMessage, MqttMessage>,
        subscriptions: TopicFilter,
    ) {
        self.subscriber_addresses
            .push((subscriptions, peer.get_response_sender()));
        peer.set_request_sender(self.publish_channel.0.clone().into())
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MqttActorBuilder {
    fn register_peer(&mut self, subscriptions: TopicFilter, sender: DynSender<MqttMessage>) {
        self.subscriber_addresses.push((subscriptions, sender));
    }
}

impl MessageSink<MqttMessage> for MqttActorBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.publish_channel.0.clone().into()
    }
}

impl Builder<(MqttActor, MqttMessageBox)> for MqttActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<(MqttActor, MqttMessageBox), Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> (MqttActor, MqttMessageBox) {
        self.build_actor_and_box()
    }
}

pub struct MqttMessageBox {
    peer_receiver: Receiver<MqttMessage>,
    peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}

impl MqttMessageBox {
    fn new(
        peer_receiver: Receiver<MqttMessage>,
        peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    ) -> Self {
        MqttMessageBox {
            peer_receiver,
            peer_senders,
        }
    }

    async fn recv(&mut self) -> Option<MqttMessage> {
        self.peer_receiver.next().await.map(|msg| {
            self.log_input(&msg);
            msg
        })
    }

    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        self.log_output(&message.clone());
        for (topic_filter, peer_sender) in self.peer_senders.iter_mut() {
            if topic_filter.accept(&message) {
                peer_sender.send(message.clone()).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl MessageBox for MqttMessageBox {
    type Input = MqttMessage;
    type Output = MqttMessage;

    fn turn_logging_on(&mut self, _on: bool) {}

    fn name(&self) -> &str {
        "MQTT"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}

pub struct MqttActor {
    mqtt_config: mqtt_channel::Config,
}

impl MqttActor {
    fn new(mqtt_config: mqtt_channel::Config) -> Self {
        MqttActor { mqtt_config }
    }
}

#[async_trait]
impl Actor for MqttActor {
    type MessageBox = MqttMessageBox;

    fn name(&self) -> &str {
        "MQTT"
    }

    async fn run(self, mut mailbox: MqttMessageBox) -> Result<(), ChannelError> {
        let mut mqtt_client = mqtt_channel::Connection::new(&self.mqtt_config)
            .await
            .unwrap(); // TODO Convert MqttError to RuntimeError;

        loop {
            tokio::select! {
                Some(message) = mailbox.recv() => {
                    mqtt_client
                    .published
                    .send(message)
                    .await
                    .expect("TODO catch actor specific errors");
                },
                Some(message) = mqtt_client.received.next() => {
                    mailbox.send(message).await?
                },
                else => break,
            }
        }
        Ok(())
    }
}
