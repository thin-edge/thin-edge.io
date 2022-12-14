use async_trait::async_trait;
use mqtt_channel::{Message, MqttError, SinkExt, StreamExt, TopicFilter};
use tedge_actors::mpsc::{channel, Receiver, Sender};
use tedge_actors::{
    Actor, ActorBuilder, ChannelError, DynSender, LinkError, MessageBox, PeerLinker, RuntimeError,
    RuntimeHandle, SimpleMessageBox,
};

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

    pub fn register_peer(
        &mut self,
        topics: TopicFilter,
    ) -> (Sender<MqttMessage>, Receiver<MqttMessage>) {
        let (sender, receiver) = channel(10);
        self.subscriber_addresses.push((topics, sender.into()));
        (self.publish_channel.0.clone(), receiver)
    }

    pub fn add_client(
        &mut self,
        subscriptions: TopicFilter,
        received_message_sender: DynSender<MqttMessage>,
    ) -> Result<DynSender<MqttMessage>, LinkError> {
        self.subscriber_addresses
            .push((subscriptions, received_message_sender));
        Ok(self.publish_channel.0.clone().into())
    }
}

impl PeerLinker<MqttMessage, MqttMessage> for MqttActorBuilder {
    fn connect(
        &mut self,
        output_sender: DynSender<MqttMessage>,
    ) -> Result<DynSender<MqttMessage>, LinkError> {
        todo!()
        // Indeed, this PeerLinker abstraction abstracts away too many things!
        // Here, we need a topic filter associated to the sender.
    }
}

#[async_trait]
impl ActorBuilder for MqttActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let mut combined_topic_filter = TopicFilter::empty();
        for (topic_filter, _) in self.subscriber_addresses.iter() {
            combined_topic_filter.add_all(topic_filter.to_owned());
        }
        let mqtt_config = self.mqtt_config.with_subscriptions(combined_topic_filter);
        let mqtt_actor = MqttActor::new(
            mqtt_config,
            self.publish_channel.1,
            self.subscriber_addresses,
        )
        .await
        .unwrap(); // Convert MqttError to RuntimeError

        runtime.run(mqtt_actor, UnusedMessageBox).await?;
        Ok(())
    }
}

struct MqttActor {
    mqtt_client: mqtt_channel::Connection,
    mailbox: Receiver<MqttMessage>,
    peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}

impl MqttActor {
    async fn new(
        mqtt_config: mqtt_channel::Config,
        mailbox: Receiver<MqttMessage>,
        peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    ) -> Result<Self, MqttError> {
        let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
        Ok(MqttActor {
            mqtt_client,
            mailbox,
            peer_senders,
        })
    }
}

struct UnusedMessageBox;

impl MessageBox for UnusedMessageBox {}

#[async_trait]
impl Actor for MqttActor {
    type MessageBox = UnusedMessageBox;

    async fn run(mut self, _unused: UnusedMessageBox) -> Result<(), ChannelError> {
        loop {
            tokio::select! {
                Some(message) = self.mailbox.next() => {
                    self.mqtt_client.published.send(message).await.expect("TODO catch actor specific errors");
                },
                Some(message) = self.mqtt_client.received.next() => {
                    for (topic_filter, peer_sender) in self.peer_senders.iter_mut() {
                        if topic_filter.accept(&message) {
                            let message = message.clone();
                            peer_sender.send(message).await?;
                        }
                    }
                },
                else => return Ok(()),
            }
        }
    }
}
