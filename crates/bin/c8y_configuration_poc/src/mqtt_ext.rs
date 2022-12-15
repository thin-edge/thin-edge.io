use async_trait::async_trait;
use mqtt_channel::{MqttError, SinkExt, StreamExt, TopicFilter};
use tedge_actors::mpsc::{channel, Receiver, Sender};
use tedge_actors::{
    Actor, ActorBuilder, ChannelError, DynSender, LinkError, MessageBox, PeerLinker, RuntimeError,
    RuntimeHandle,
};

pub type MqttMessage = mqtt_channel::Message;

pub enum MqttActorMessage {
    DirectMessage(mqtt_channel::Message),
    PeerMessage(mqtt_channel::Message),
}

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

    // This method makes explicit this actor can consume MqttMessage and send MqttMessage.
    // However, returning a Receiver where the peer will receive the messages received from its subscription
    // put the burden on the receiving peer which has no more freedom on organising its channel.
    // pub fn register_peer(
    //     &mut self,
    //     topics: TopicFilter,
    // ) -> (Sender<MqttMessage>, Receiver<MqttMessage>) {
    //     let (sender, receiver) = channel(10);
    //     self.subscriber_addresses.push((topics, sender.into()));
    //     (self.publish_channel.0.clone(), receiver)
    // }

    pub fn add_client(
        &mut self,
        subscriptions: TopicFilter,
        peer_sender: DynSender<MqttMessage>,
    ) -> Result<DynSender<MqttMessage>, LinkError> {
        self.subscriber_addresses.push((subscriptions, peer_sender));
        Ok(self.publish_channel.0.clone().into())
    }
}

impl PeerLinker<MqttMessage, MqttMessage> for MqttActorBuilder {
    fn connect(
        &mut self,
        _output_sender: DynSender<MqttMessage>,
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
        let mqtt_message_box = MqttMessageBox::new(
            mqtt_config,
            self.publish_channel.1,
            self.subscriber_addresses,
        )
        .await
        .unwrap(); // Convert MqttError to RuntimeError

        let mqtt_actor = MqttActor;

        runtime.run(mqtt_actor, mqtt_message_box).await?;
        Ok(())
    }
}

struct MqttMessageBox {
    mqtt_client: mqtt_channel::Connection,
    peer_receiver: Receiver<MqttMessage>,
    peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
}
impl MessageBox for MqttMessageBox {}

impl MqttMessageBox {
    async fn new(
        mqtt_config: mqtt_channel::Config,
        peer_receiver: Receiver<MqttMessage>,
        peer_senders: Vec<(TopicFilter, DynSender<MqttMessage>)>,
    ) -> Result<Self, MqttError> {
        let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
        Ok(MqttMessageBox {
            mqtt_client,
            peer_receiver,
            peer_senders,
        })
    }

    pub async fn recv(&mut self) -> MqttActorMessage {
        tokio::select! {
            Some(message) = self.peer_receiver.next() => {
                MqttActorMessage::PeerMessage(message)
            },
            Some(message) = self.mqtt_client.received.next() => {
                MqttActorMessage::DirectMessage(message)
            },
        }
    }

    pub async fn send(&mut self, message: MqttActorMessage) -> Result<(), ChannelError> {
        match message {
            MqttActorMessage::DirectMessage(message) => {
                for (topic_filter, peer_sender) in self.peer_senders.iter_mut() {
                    if topic_filter.accept(&message) {
                        let message = message.clone();
                        peer_sender.send(message).await?;
                    }
                }
            }
            MqttActorMessage::PeerMessage(message) => {
                self.mqtt_client
                    .published
                    .send(message)
                    .await
                    .expect("TODO catch actor specific errors");
            }
        }
        Ok(())
    }
}

struct MqttActor;

#[async_trait]
impl Actor for MqttActor {
    type MessageBox = MqttMessageBox;

    async fn run(mut self, mut mailbox: MqttMessageBox) -> Result<(), ChannelError> {
        loop {
            let message = mailbox.recv().await;
            mailbox.send(message).await?;
        }
    }
}
