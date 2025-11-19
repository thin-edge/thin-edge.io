use crate::Config;
use crate::ErrChannel;
use crate::MqttError;
use crate::MqttMessage;
use crate::PubChannel;
use crate::SubChannel;
use crate::TopicFilter;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::SinkExt;
use futures::StreamExt;
use log::error;
use log::info;
use log::warn;
use rumqttc::AsyncClient;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::SubscribeFilter;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// A connection to some MQTT server
pub struct Connection {
    /// The channel of the input messages received by this connection.
    pub received: mpsc::UnboundedReceiver<MqttMessage>,

    /// The channel of the output messages to be published on this connection.
    pub published: mpsc::UnboundedSender<MqttMessage>,

    /// The channel of the error messages received by this connection.
    pub errors: mpsc::UnboundedReceiver<MqttError>,

    /// A channel to notify that all the published messages have been actually published.
    pub pub_done: oneshot::Receiver<()>,

    pub subscriptions: SubscriberHandle,
}

#[derive(Clone)]
/// A client for changing the subscribed topics
pub struct SubscriberHandle {
    client: AsyncClient,
    pub(crate) subscriptions: Arc<Mutex<TopicFilter>>,
}

impl SubscriberHandle {
    pub fn new(client: AsyncClient, subscriptions: Arc<Mutex<TopicFilter>>) -> Self {
        Self {
            client,
            subscriptions,
        }
    }
}

#[async_trait::async_trait]
pub trait SubscriberOps {
    async fn subscribe_many(
        &self,
        topics: impl IntoIterator<Item = String> + Send,
    ) -> Result<(), MqttError>;
    async fn unsubscribe_many(
        &self,
        topics: impl IntoIterator<Item = String> + Send,
    ) -> Result<(), MqttError>;
}

#[async_trait::async_trait]
impl SubscriberOps for SubscriberHandle {
    async fn subscribe_many(
        &self,
        topics: impl IntoIterator<Item = String> + Send,
    ) -> Result<(), MqttError> {
        let topics = topics.into_iter().collect::<Vec<_>>();
        {
            let mut subs = self.subscriptions.lock().unwrap();
            for topic in &topics {
                subs.try_add(topic)?;
            }
        }
        self.client
            .subscribe_many(topics.into_iter().map(|path| SubscribeFilter {
                path,
                qos: rumqttc::QoS::AtLeastOnce,
            }))
            .await?;
        Ok(())
    }

    async fn unsubscribe_many(
        &self,
        topics: impl IntoIterator<Item = String> + Send,
    ) -> Result<(), MqttError> {
        let topics = topics.into_iter().collect::<Vec<_>>();
        {
            let mut subs = self.subscriptions.lock().unwrap();
            for topic in &topics {
                subs.remove(topic);
            }
        }
        for topic in topics {
            self.client.unsubscribe(topic).await?;
        }
        Ok(())
    }
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
        let subscriptions = Arc::new(Mutex::new(config.subscriptions.clone()));

        let (mqtt_client, event_loop) = Connection::open(
            config,
            received_sender.clone(),
            error_sender.clone(),
            subscriptions.clone(),
        )
        .await?;
        let permits = Arc::new(Semaphore::new(1));
        let permit = permits.clone().acquire_owned().await.unwrap();
        let pub_count = Arc::new(AtomicUsize::new(0));
        tokio::spawn(Connection::receiver_loop(
            mqtt_client.clone(),
            config.clone(),
            event_loop,
            received_sender,
            error_sender.clone(),
            pub_done_sender,
            permits,
            pub_count.clone(),
            subscriptions.clone(),
        ));
        tokio::spawn(Connection::sender_loop(
            mqtt_client.clone(),
            published_receiver,
            error_sender,
            config.last_will_message.clone(),
            permit,
            pub_count,
        ));

        Ok(Connection {
            received: received_receiver,
            published: published_sender,
            errors: error_receiver,
            pub_done: pub_done_receiver,
            subscriptions: SubscriberHandle::new(mqtt_client, subscriptions),
        })
    }

    pub async fn close(self) {
        self.published.close_channel();
        let _ = self.pub_done.await;
    }

    async fn open(
        config: &Config,
        mut message_sender: mpsc::UnboundedSender<MqttMessage>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
        subscriptions: Arc<Mutex<TopicFilter>>,
    ) -> Result<(AsyncClient, EventLoop), MqttError> {
        const INSECURE_MQTT_PORT: u16 = 1883;
        const SECURE_MQTT_PORT: u16 = 8883;

        if config.broker.port == INSECURE_MQTT_PORT && config.broker.is_using_tls() {
            warn!(target: "MQTT", "Connecting on port 1883 for insecure MQTT using a TLS connection");
        }
        if config.broker.port == SECURE_MQTT_PORT && !config.broker.is_using_tls() {
            warn!(target: "MQTT", "Connecting on port 8883 for secure MQTT without a CA file");
        }

        let mqtt_options = config.rumqttc_options()?;
        let (mqtt_client, mut event_loop) = AsyncClient::new(mqtt_options, config.queue_capacity);

        info!(target: "MQTT",
            "Connecting to broker: host={}:{}, session_name={:?}",
            config.broker.host, config.broker.port, config.session_name
        );

        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if let Some(err) = MqttError::maybe_connection_error(&ack) {
                        return Err(err);
                    };
                    info!(target: "MQTT", "Connection established");

                    let subscriptions = subscriptions.lock().unwrap().filters();

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
                    if msg.payload.len() > config.max_packet_size {
                        error!(target: "MQTT", "Dropping message received on topic {} with payload size {} that exceeds the maximum packet size of {}",
                            msg.topic, msg.payload.len(), config.max_packet_size);
                        continue;
                    }
                    let _ = message_sender.send(msg.into()).await;
                }

                Err(err) => {
                    error!(target: "MQTT",
                        "Failed to connect to broker at '{host}:{port}': {err}",
                        host = config.broker.host,
                        port = config.broker.port
                    );

                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = error_sender.send(err.into()).await;

                    Connection::do_pause().await;
                }
                _ => (),
            }
        }

        Ok((mqtt_client, event_loop))
    }

    #[allow(clippy::too_many_arguments)]
    async fn receiver_loop(
        mqtt_client: AsyncClient,
        config: Config,
        mut event_loop: EventLoop,
        mut message_sender: mpsc::UnboundedSender<MqttMessage>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
        done: oneshot::Sender<()>,
        permits: Arc<Semaphore>,
        pub_count: Arc<AtomicUsize>,
        subscriptions: Arc<Mutex<TopicFilter>>,
    ) -> Result<(), MqttError> {
        let mut triggered_disconnect = false;
        let mut disconnect_permit = None;
        let mut awaiting_ack = HashSet::new();

        loop {
            // Check if we are ready to disconnect. Due to ownership of the
            // event loop, this needs to be done before we call
            // `event_loop.poll()`
            let remaining_events_empty =
                // Are there any queued publishes?
                pub_count.load(Ordering::SeqCst) == 0&&
                // And are any published messages awaiting acknowledgement?
                event_loop.state.inflight() == 0;
            if disconnect_permit.is_some() && !triggered_disconnect && remaining_events_empty {
                // `sender_loop` is not running and we have no remaining
                // publishes to process
                let client = mqtt_client.clone();
                tokio::spawn(async move { client.disconnect().await });
                triggered_disconnect = true;
            }

            let event = tokio::select! {
                // If there is an event, we need to process that first
                // Otherwise we risk shutting down early
                // e.g. a `Publish` request from the sender is not "inflight"
                // but will immediately be returned by `event_loop.poll()`
                biased;

                event = event_loop.poll() => event,
                permit = permits.clone().acquire_owned() => {
                    // The `sender_loop` has now concluded
                    disconnect_permit = Some(permit.unwrap());
                    continue;
                }
            };

            match event {
                Ok(Event::Incoming(Packet::Publish(msg))) => {
                    if msg.payload.len() > config.max_packet_size {
                        error!(target: "MQTT", "Dropping message received on topic {} with payload size {} that exceeds the maximum packet size of {}",
                            msg.topic, msg.payload.len(), config.max_packet_size);
                        continue;
                    }
                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    // One has to continue the loop though, because rumqttc relies on this polling.
                    let _ = message_sender.send(msg.into()).await;
                }

                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if let Some(err) = MqttError::maybe_connection_error(&ack) {
                        error!(target: "MQTT", "Connection Error {err}");
                    } else {
                        info!(target: "MQTT", "Connection re-established");
                        if let Some(ref imsg_fn) = config.initial_message {
                            // publish the initial message on connect
                            let message = imsg_fn.new_init_message();
                            mqtt_client
                                .publish(
                                    message.topic.name.clone(),
                                    message.qos,
                                    message.retain,
                                    message.payload_bytes().to_vec(),
                                )
                                .await?;
                        }

                        if config.session_name.is_none() || !ack.session_present {
                            // Workaround for  https://github.com/bytebeamio/rumqtt/issues/250
                            // If session_name is not provided or if the broker session persistence
                            // is not enabled or working, then re-subscribe

                            let subscriptions = subscriptions.lock().unwrap().filters();
                            // Need check here otherwise it will hang waiting for a SubAck, and none will come when there is no subscription.
                            if subscriptions.is_empty() {
                                break;
                            }
                            Connection::subscribe_to_topics(&mqtt_client, subscriptions).await?;
                        }
                    }
                }

                Ok(Event::Incoming(Packet::Disconnect))
                | Ok(Event::Outgoing(Outgoing::Disconnect)) => {
                    break;
                }

                Ok(Event::Outgoing(Outgoing::Publish(p))) => {
                    if !awaiting_ack.contains(&p) {
                        pub_count.fetch_sub(1, Ordering::SeqCst);
                    }
                    awaiting_ack.insert(p);
                }

                Ok(Event::Incoming(Packet::PubAck(p))) => {
                    awaiting_ack.remove(&p.pkid);
                }

                Ok(Event::Incoming(Packet::PubComp(p))) => {
                    awaiting_ack.remove(&p.pkid);
                }

                Err(err) => {
                    error!(target: "MQTT", "Connection error: {err}");

                    // Errors on send are ignored: it just means the client has closed the receiving channel.
                    let _ = error_sender.send(err.into()).await;

                    Connection::do_pause().await;
                }
                _ => (),
            }
        }

        // Wait for Err(MqttState(ConnectionAborted))
        // to make sure the disconnect is effective
        loop {
            if (event_loop.poll().await).is_err() {
                info!(target: "MQTT", "Connection closed");
                break;
            }
        }

        // No more messages will be forwarded to the client
        let _ = message_sender.close().await;
        let _ = error_sender.close().await;
        let _ = done.send(());
        Ok(())
    }

    async fn sender_loop(
        mqtt_client: AsyncClient,
        mut messages_receiver: mpsc::UnboundedReceiver<MqttMessage>,
        mut error_sender: mpsc::UnboundedSender<MqttError>,
        last_will: Option<MqttMessage>,
        _disconnect_permit: OwnedSemaphorePermit,
        pub_count: Arc<AtomicUsize>,
    ) {
        while let Some(message) = messages_receiver.next().await {
            let payload = Vec::from(message.payload_bytes());
            if let Err(err) = mqtt_client
                .publish(message.topic, message.qos, message.retain, payload)
                .await
            {
                let _ = error_sender.send(err.into()).await;
            } else {
                pub_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        // As the broker doesn't send the last will when the client disconnects gracefully
        // one has first to explicitly send the last will message.
        if let Some(last_will) = last_will {
            let payload = Vec::from(last_will.payload_bytes());
            if mqtt_client
                .publish(last_will.topic, last_will.qos, last_will.retain, payload)
                .await
                .is_ok()
            {
                pub_count.fetch_add(1, Ordering::SeqCst);
            }
        }

        // At this point, `_disconnect_permit` is dropped
        // This allows `receiver_loop` acquire a permit and commence the shutdown process
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
