use crate::{Config, Message, MqttError, PubChannel, SubChannel, TopicFilter};
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use rumqttc::{
    AsyncClient, ConnectionError, Event, EventLoop, Incoming, Outgoing, Packet, StateError,
};
use std::time::Duration;
use tokio::time::sleep;

/// A connection to some MQTT server
pub struct Connection {
    /// The channel of the input messages received by this connection.
    pub received: mpsc::UnboundedReceiver<Message>,

    /// The channel of the output messages to be published on this connection.
    pub published: mpsc::UnboundedSender<Message>,
}

impl Connection {
    /// The stream of events received from this MQTT connected and forwarded to the client
    pub fn sub_channel(&self) -> &impl SubChannel {
        &self.received
    }

    /// The stream of actions sent by the client to this MQTT connection
    pub fn pub_channel(&self) -> &impl PubChannel {
        &self.published
    }

    /// Establish a connection to the MQTT broker defined by the given `config`.
    ///
    /// This connection is associated to an MQTT session with the given `name` and `subscription`.
    ///
    /// Reusing the same session name on each connection allows a client
    /// to have its subscription persisted by the broker
    /// so messages sent while the client is disconnected are received on re-connection.
    ///
    /// The connection will only receive messages published on the subscription topics.
    ///
    /// ```no_run
    /// # use mqtt_channel::{Config, Connection, Topic, MqttError};
    /// # use std::convert::TryInto;
    ///
    /// # #[tokio::main]
    /// # async fn connect() -> Result<Connection, MqttError> {
    ///     // A client can subscribe to many topics
    ///     let topics = vec![
    ///         "/a/first/topic",
    ///         "/a/second/topic",
    ///         "/a/+/pattern", // one can use + pattern
    ///         "/any/#",       // one can use # pattern
    ///     ]
    ///     .try_into()
    ///     .expect("a list of topic filters");
    ///
    ///     Connection::connect("test", &Config::default(), topics).await
    /// # }
    pub async fn connect(
        name: &str,
        config: &Config,
        subscription: TopicFilter,
    ) -> Result<Connection, MqttError> {
        let (received_sender, received_receiver) = mpsc::unbounded();
        let (published_sender, published_receiver) = mpsc::unbounded();
        let init_message_sender = received_sender.clone();

        let (mqtt_client, event_loop) =
            Connection::open(name, config, subscription, init_message_sender).await?;
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
        mqtt_options.set_max_packet_size(config.max_packet_size, config.max_packet_size);

        mqtt_options
    }

    async fn open(
        name: &str,
        config: &Config,
        topic: TopicFilter,
        mut message_sender: mpsc::UnboundedSender<Message>,
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
                    let _ = message_sender.send(msg.into()).await;
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
        mut message_sender: mpsc::UnboundedSender<Message>,
    ) {
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    let _ = message_sender.send(msg.into()).await;
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
        let _ = message_sender.close().await;
    }

    async fn sender_loop(
        mqtt_client: AsyncClient,
        mut messages_receiver: mpsc::UnboundedReceiver<Message>,
    ) {
        loop {
            match messages_receiver.next().await {
                None => {
                    // The sender channel has been closed by the client
                    // No more messages will be published by the client
                    break;
                }
                Some(message) => {
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
