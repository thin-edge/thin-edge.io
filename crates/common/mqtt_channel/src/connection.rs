use crate::Config;
use crate::ErrChannel;
use crate::Message;
use crate::MqttError;
use crate::PubChannel;
use crate::SubChannel;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::Packet;
use rumqttc::StateError;
use std::time::Duration;
use tokio::time::sleep;

/// A connection to some MQTT server
pub struct Connection {
    /// The channel of the input messages received by this connection.
    pub received: mpsc::UnboundedReceiver<Message>,

    /// The channel of the output messages to be published on this connection.
    pub published: mpsc::UnboundedSender<Message>,

    /// The channel of the error messages received by this connection.
    pub errors: mpsc::UnboundedReceiver<MqttError>,

    /// A channel to notify that all the published messages have been actually published.
    pub pub_done: oneshot::Receiver<()>,
}

impl Connection {
    /// The stream of events received from this MQTT connection and forwarded to the client
    pub fn sub_channel(&self) -> &impl SubChannel {
        &self.received
    }

    /// The stream of actions sent by the client to this MQTT connection
    pub fn pub_channel(&self) -> &impl PubChannel {
        &self.published
    }

    /// The stream of errors received from this MQTT connection and forwarded to the client
    pub fn err_channel(&self) -> &impl ErrChannel {
        &self.errors
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
    ///     let config = Config::default().with_session_name("test").with_subscriptions(topics);
    ///
    ///     Connection::new(&config).await
    /// # }
    pub async fn new(config: &Config) -> Result<Connection, MqttError> {
        let (received_sender, received_receiver) = mpsc::unbounded();
        let (published_sender, published_receiver) = mpsc::unbounded();
        let (error_sender, error_receiver) = mpsc::unbounded();
        let (pub_done_sender, pub_done_receiver) = oneshot::channel();

        let (mqtt_client, event_loop) =
            Connection::open(config, received_sender.clone(), error_sender.clone()).await?;
        tokio::spawn(Connection::receiver_loop(
            mqtt_client.clone(),
            config.clone(),
            event_loop,
            received_sender,
            error_sender.clone(),
        ));
        tokio::spawn(Connection::sender_loop(
            mqtt_client,
            published_receiver,
            error_sender,
            pub_done_sender,
        ));

        Ok(Connection {
            received: received_receiver,
            published: published_sender,
            errors: error_receiver,
            pub_done: pub_done_receiver,
        })
    }

    pub async fn close(self) {
        self.published.close_channel();
        let _ = self.pub_done.await;
    }

    async fn open(
        config: &Config,
        mut message_sender: mpsc::UnboundedSender<Message>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
    ) -> Result<(AsyncClient, EventLoop), MqttError> {
        let mqtt_options = config.mqtt_options();
        let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, config.queue_capacity);

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if let Some(err) = MqttError::maybe_connection_error(&ack) {
                        return Err(err);
                    };
                    let subscriptions = config.subscriptions.filters();

                    // Need check here otherwise it will hang waiting for a SubAck, and none will come when there is no subscription.
                    if subscriptions.is_empty() {
                        break;
                    }

                    Connection::subscribe_to_topics(&mqtt_client, subscriptions).await?
                }

                Ok(Event::Incoming(Packet::SubAck(ack))) => {
                    if let Some(err) = MqttError::maybe_subscription_error(&ack) {
                        return Err(err);
                    };
                    break;
                }

                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    // Messages can be received before a sub ack
                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = message_sender.send(msg.into()).await;
                }

                Err(err) => {
                    let should_delay = Connection::pause_on_error(&err);

                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = error_sender.send(err.into()).await;

                    if should_delay {
                        Connection::do_pause().await;
                    }
                }
                _ => (),
            }
        }

        Ok((mqtt_client, event_loop))
    }

    async fn receiver_loop(
        mqtt_client: AsyncClient,
        config: Config,
        mut event_loop: EventLoop,
        mut message_sender: mpsc::UnboundedSender<Message>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
    ) -> Result<(), MqttError> {
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    // One has to continue the loop though, because rumqttc relies on this polling.
                    let _ = message_sender.send(msg.into()).await;
                }

                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if let Some(err) = MqttError::maybe_connection_error(&ack) {
                        eprintln!("ERROR: Connection Error {}", err);
                    }
                    // Workaround for  https://github.com/bytebeamio/rumqtt/issues/250
                    // If session_name is not provided, then re-subscribe
                    else if config.session_name.is_none() {
                        let subscriptions = config.subscriptions.filters();
                        // Need check here otherwise it will hang waiting for a SubAck, and none will come when there is no subscription.
                        if subscriptions.is_empty() {
                            break;
                        }
                        Connection::subscribe_to_topics(&mqtt_client, subscriptions).await?;
                    }
                }

                Ok(Event::Incoming(Incoming::Disconnect)) => {
                    // The connection has been closed
                    break;
                }

                Err(err) => {
                    let delay = Connection::pause_on_error(&err);

                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = error_sender.send(err.into()).await;

                    if delay {
                        Connection::do_pause().await;
                    }
                }
                _ => (),
            }
        }
        // No more messages will be forwarded to the client
        let _ = message_sender.close().await;
        let _ = error_sender.close().await;
        Ok(())
    }

    async fn sender_loop(
        mqtt_client: AsyncClient,
        mut messages_receiver: mpsc::UnboundedReceiver<Message>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
        done: oneshot::Sender<()>,
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
                        let _ = error_sender.send(err.into()).await;
                    }
                }
            }
        }
        let _ = mqtt_client.disconnect().await;
        let _ = done.send(());
    }

    pub(crate) fn pause_on_error(err: &ConnectionError) -> bool {
        match &err {
            rumqttc::ConnectionError::Io(_) => true,
            rumqttc::ConnectionError::MqttState(state_error)
                if matches!(state_error, StateError::Io(_)) =>
            {
                true
            }
            rumqttc::ConnectionError::MqttState(_) => true,
            _ => false,
        }
    }

    pub(crate) async fn do_pause() {
        sleep(Duration::from_secs(1)).await;
    }

    pub(crate) async fn subscribe_to_topics(
        mqtt_client: &AsyncClient,
        subscriptions: Vec<rumqttc::SubscribeFilter>,
    ) -> Result<(), MqttError> {
        mqtt_client
            .subscribe_many(subscriptions)
            .await
            .map_err(MqttError::ClientError)
    }
}
