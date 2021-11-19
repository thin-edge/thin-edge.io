use crate::{Config, Message, MqttError, TopicFilter};
use rumqttc::{
    AsyncClient, ConnectionError, Event, EventLoop, Incoming, Outgoing, Packet, StateError,
};
use std::time::Duration;
use tokio::time::sleep;

/// A connection to some MQTT server
pub struct Connection {
    /// The channel of the input messages received by this connection.
    pub received: async_broadcast::Receiver<Message>,

    /// The channel of the output messages to be published on this connection.
    pub published: async_channel::Sender<Message>,
}

impl Connection {
    pub async fn connect(
        name: &str,
        config: &Config,
        topic: TopicFilter,
    ) -> Result<Connection, MqttError> {
        let (received_sender, received_receiver) =
            async_broadcast::broadcast(config.queue_capacity);
        let (published_sender, published_receiver) = async_channel::unbounded();
        let init_message_sender = received_sender.clone();

        let (mqtt_client, event_loop) =
            Connection::open(name, config, topic, init_message_sender).await?;
        tokio::spawn(Connection::receiver_loop(event_loop, received_sender));
        tokio::spawn(Connection::sender_loop(mqtt_client, published_receiver));

        Ok(Connection {
            received: received_receiver,
            published: published_sender,
        })
    }

    fn mqtt_options(name: &str, config: &Config) -> rumqttc::MqttOptions {
        let mut mqtt_options = rumqttc::MqttOptions::new(name, &config.host, config.port);
        mqtt_options.set_clean_session(config.clean_session);

        mqtt_options
    }

    async fn open(
        name: &str,
        config: &Config,
        topic: TopicFilter,
        message_sender: async_broadcast::Sender<Message>,
    ) -> Result<(AsyncClient, EventLoop), MqttError> {
        let mqtt_options = Connection::mqtt_options(name, config);
        let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, config.queue_capacity);

        let qos = topic.qos;

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    if topic.patterns.is_empty() {
                        break;
                    }

                    for pattern in topic.patterns.iter() {
                        let () = mqtt_client.subscribe(pattern, qos).await?;
                    }
                }

                Ok(Event::Incoming(Packet::SubAck(_))) => {
                    break;
                }

                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    // Messages can be received before a sub ack
                    let _ = message_sender.broadcast(msg.into()).await;
                }

                Err(err) => {
                    eprintln!("ERROR: {}", err);
                    Connection::pause_on_error(err).await;
                }
                _ => (),
            }
        }

        Ok((mqtt_client, event_loop))
    }

    async fn receiver_loop(
        mut event_loop: EventLoop,
        message_sender: async_broadcast::Sender<Message>,
    ) {
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    let _ = message_sender.broadcast(msg.into()).await;
                }

                Ok(Event::Incoming(Incoming::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    // The connection has been closed
                    break;
                }

                Err(err) => {
                    eprintln!("ERROR: {}", err);
                    Connection::pause_on_error(err).await;
                }
                _ => (),
            }
        }
        // No more messages will be forwarded to the client
        message_sender.close();
    }

    async fn sender_loop(
        mqtt_client: AsyncClient,
        messages_receiver: async_channel::Receiver<Message>,
    ) {
        loop {
            match messages_receiver.recv().await {
                Err(async_channel::RecvError) => {
                    // The sender channel has been drop
                    break;
                }
                Ok(message) => {
                    let payload = Vec::from(message.payload_bytes());
                    if let Err(err) = mqtt_client
                        .publish(message.topic, message.qos, message.retain, payload)
                        .await
                    {
                        eprintln!("ERROR: Fail to publish a message: {}", err);
                    }
                }
            }
        }
        let _ = mqtt_client.disconnect().await;
    }

    async fn pause_on_error(err: ConnectionError) {
        let delay = match &err {
            rumqttc::ConnectionError::Io(_) => true,
            rumqttc::ConnectionError::MqttState(state_error)
                if matches!(state_error, StateError::Io(_)) =>
            {
                true
            }
            rumqttc::ConnectionError::MqttState(_) => true,
            rumqttc::ConnectionError::Mqtt4Bytes(_) => true,
            _ => false,
        };

        if delay {
            sleep(Duration::from_secs(1)).await;
        }
    }
}
