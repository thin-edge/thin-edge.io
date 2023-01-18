#[cfg(test)]
mod tests;

use async_trait::async_trait;
use mqtt_channel::MqttError;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use tedge_actors::mpsc;
use tedge_actors::mpsc::channel;
use tedge_actors::mpsc::Receiver;
use tedge_actors::mpsc::Sender;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageBox;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::SimpleMessageBox;

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

    pub fn add_client(
        &mut self,
        subscriptions: TopicFilter,
        peer_sender: DynSender<MqttMessage>,
    ) -> Result<DynSender<MqttMessage>, LinkError> {
        self.subscriber_addresses.push((subscriptions, peer_sender));
        Ok(self.publish_channel.0.clone().into())
    }

    /// Add a new client, returning a message box to pub/sub messages over MQTT
    pub fn new_client(
        &mut self,
        client_name: &str,
        subscriptions: TopicFilter,
    ) -> SimpleMessageBox<MqttMessage, MqttMessage> {
        let (sub_message_sender, sub_message_receiver) = mpsc::channel(16);
        let pub_message_sender = self
            .add_client(subscriptions, sub_message_sender.into())
            .unwrap();

        SimpleMessageBox::new(
            client_name.to_string(),
            sub_message_receiver,
            pub_message_sender,
        )
    }

    /// FIXME this method should not be async
    pub(crate) async fn build(self) -> (MqttActor, MqttMessageBox) {
        let mut combined_topic_filter = TopicFilter::empty();
        for (topic_filter, _) in self.subscriber_addresses.iter() {
            combined_topic_filter.add_all(topic_filter.to_owned());
        }
        let mqtt_config = self.mqtt_config.with_subscriptions(combined_topic_filter);
        let mqtt_message_box = MqttMessageBox::new(
            self.publish_channel.1,
            self.subscriber_addresses,
        );

        let mqtt_actor = MqttActor::new(mqtt_config)
            .await
            .unwrap(); // Convert MqttError to RuntimeError;
        (mqtt_actor, mqtt_message_box)
    }
}

#[async_trait]
impl ActorBuilder for MqttActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let (mqtt_actor, mqtt_message_box) = self.build().await;

        runtime.run(mqtt_actor, mqtt_message_box).await?;
        Ok(())
    }
}

struct MqttMessageBox {
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
}

#[async_trait]
impl MessageBox for MqttMessageBox {
    type Input = MqttMessage;
    type Output = MqttMessage;

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

    fn turn_logging_on(&mut self, _on: bool) {}

    fn name(&self) -> &str {
        "MQTT"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}

struct MqttActor {
    mqtt_client: mqtt_channel::Connection,
}

impl MqttActor {
    async fn new(
        mqtt_config: mqtt_channel::Config,
    ) -> Result<Self, MqttError> {
        let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
        Ok(MqttActor {
            mqtt_client,
        })
    }
}

#[async_trait]
impl Actor for MqttActor {
    type MessageBox = MqttMessageBox;

    fn name(&self) -> &str {
        "MQTT"
    }

    async fn run(mut self, mut mailbox: MqttMessageBox) -> Result<(), ChannelError> {
        loop {
            tokio::select! {
                Some(message) = mailbox.recv() => {
                    self.mqtt_client
                    .published
                    .send(message)
                    .await
                    .expect("TODO catch actor specific errors");
                },
                Some(message) = self.mqtt_client.received.next() => {
                    mailbox.send(message).await?
                },
                else => break,
            }
        }
        Ok(())
    }
}
