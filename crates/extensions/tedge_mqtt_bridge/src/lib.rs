mod backoff;
mod config;
mod health;
#[cfg(test)]
mod test_helpers;
mod topics;

use async_trait::async_trait;
use bytes::Bytes;
pub use rumqttc;
use rumqttc::AsyncClient;
use rumqttc::ClientError;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::LastWill;
pub use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::PubAck;
use rumqttc::PubRec;
use rumqttc::Publish;
use rumqttc::Request;
use rumqttc::SubscribeFilter;
use rumqttc::Transport;
use std::borrow::Cow;
use std::collections::hash_map;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::NullSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;

pub type MqttConfig = mqtt_channel::Config;

use crate::health::BridgeHealth;
use crate::health::BridgeHealthMonitor;
pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::MqttMessage;
pub use mqtt_channel::QoS;
pub use mqtt_channel::Topic;
use tedge_config::tedge_toml::TEdgeConfigReaderMqttBridgeReconnectPolicy;
use tedge_config::TEdgeConfig;

use crate::backoff::CustomBackoff;
use crate::topics::matches_ignore_dollar_prefix;
use crate::topics::TopicConverter;
pub use config::*;

const MAX_PACKET_SIZE: usize = 268435455; // maximum allowed MQTT payload size

pub struct MqttBridgeActorBuilder {}

impl MqttBridgeActorBuilder {
    // XXX(marcel): this function loads certs, which can fail, so it should probably be fallible
    pub async fn new(
        tedge_config: &TEdgeConfig,
        service_name: &str,
        health_topic: &Topic,
        rules: BridgeConfig,
        mut cloud_config: MqttOptions,
    ) -> Self {
        let mut local_config = MqttOptions::new(
            service_name,
            &tedge_config.mqtt.client.host,
            tedge_config.mqtt.client.port.into(),
        );
        // TODO cope with certs but not ca_dir, or handle that case with an explicit error message?
        let auth_config = tedge_config.mqtt_client_auth_config();
        let local_tls_config = auth_config.to_rustls_client_config().unwrap();
        if let Some(tls_config) = local_tls_config {
            local_config.set_transport(Transport::tls_with_config(tls_config.into()));
        }
        local_config.set_max_packet_size(MAX_PACKET_SIZE, MAX_PACKET_SIZE);
        local_config.set_manual_acks(true);
        local_config.set_last_will(LastWill::new(
            &health_topic.name,
            Status::Down.json(),
            QoS::AtLeastOnce,
            true,
        ));
        local_config.set_clean_session(false);

        let reconnect_policy = tedge_config.mqtt.bridge.reconnect_policy.clone();

        cloud_config.set_manual_acks(true);
        cloud_config.set_max_packet_size(MAX_PACKET_SIZE, MAX_PACKET_SIZE);

        // When configured with a low max inflight count of messages, rumqttc might reuse the pkid of message not acknowledged yet
        // leading to the confusing messages:
        // 2024-09-10T16:13:23.497043857Z  INFO rumqttc::state: Collision on packet id = 1
        // 2024-09-10T16:13:23.497072791Z  INFO tedge_mqtt_bridge: Received notification (cloud) Outgoing(AwaitAck(1))
        // 2024-09-10T16:13:23.497100479Z  INFO tedge_mqtt_bridge: Bridge cloud connection still waiting ack for pkid=1
        // 2024-09-10T16:13:23.608007183Z  INFO tedge_mqtt_bridge: Received notification (cloud) Outgoing(Publish(1))
        // 2024-09-10T16:13:23.608233911Z  INFO tedge_mqtt_bridge: Bridge cloud connection ignoring already known pkid=1
        //
        // To prevent that, rumqttc inflight is set far bigger than the number of expected inflight messages.
        let in_flight: u16 = 100;
        cloud_config.set_inflight(in_flight * 5);
        let (local_client, local_event_loop) = AsyncClient::new(local_config, in_flight.into());
        let (cloud_client, cloud_event_loop) = AsyncClient::new(cloud_config, in_flight.into());

        let local_topics: Vec<_> = rules
            .local_subscriptions()
            .map(|t| SubscribeFilter::new(t.to_owned(), QoS::AtLeastOnce))
            .collect();
        let cloud_topics: Vec<_> = rules
            .remote_subscriptions()
            .map(|t| SubscribeFilter::new(t.to_owned(), QoS::AtLeastOnce))
            .collect();

        let [cloud_target, local_target] =
            bidirectional_channel(cloud_client.clone(), local_client.clone(), in_flight.into());
        let [(convert_local, bidir_local), (convert_cloud, bidir_cloud)] =
            rules.converters_and_bidirectional_topic_filters();
        let (tx_status, monitor) =
            BridgeHealthMonitor::new(health_topic.name.clone(), &local_target);
        tokio::spawn(monitor.monitor());
        tokio::spawn(half_bridge(
            local_event_loop,
            local_client,
            cloud_target,
            convert_local,
            bidir_local,
            tx_status.clone(),
            "local",
            local_topics,
            reconnect_policy.clone(),
        ));
        tokio::spawn(half_bridge(
            cloud_event_loop,
            cloud_client,
            local_target,
            convert_cloud,
            bidir_cloud,
            tx_status.clone(),
            "cloud",
            cloud_topics,
            reconnect_policy,
        ));

        Self {}
    }

    pub(crate) fn build_actor(self) -> MqttBridgeActor {
        MqttBridgeActor {}
    }
}

fn bidirectional_channel<Client: MqttClient + 'static>(
    cloud_client: Client,
    local_client: Client,
    buffer: usize,
) -> [BridgeAsyncClient<Client>; 2] {
    let (tx_first, rx_first) = mpsc::channel(buffer);
    let (tx_second, rx_second) = mpsc::channel(buffer);
    [
        BridgeAsyncClient::new(cloud_client, tx_first, rx_second),
        BridgeAsyncClient::new(local_client, tx_second, rx_first),
    ]
}

enum BridgeMessage {
    /// A message to be published to a given target topic
    ///
    /// This message will have to be acknowledged to its source by the companion half bridge
    BridgePub {
        target_topic: String,
        publish: Publish,
    },

    /// A message to be acknowledged on the target
    ///
    /// This message has been received by the companion half bridge
    BridgeAck { publish: Publish },

    /// A message *generated* by the bridge
    ///
    /// This message has not to be acknowledged, as not received by the bridge.
    Pub { publish: Publish },
}

/// Wraps the target of an half bridge with a channel to its half bridge companion.
///
/// So when a message is received and published by this half,
/// the companion will await for that message to be acknowledged by the target
/// before acknowledging to the source.
struct BridgeAsyncClient<Client: MqttClient> {
    /// MQTT target for the messages
    target: Client,

    /// Receives messages from the companion half bridge
    rx: mpsc::Receiver<Option<(String, Publish)>>,

    /// Sends messages to a background task that forwards the messages to the target and companion
    sender: BridgeMessageSender,

    /// Count of messages that have been published (excluding health messages)
    published: Arc<AtomicUsize>,

    /// Count of messages that have been acknowledged
    acknowledged: Arc<AtomicUsize>,
}

impl<Client: MqttClient + 'static> BridgeAsyncClient<Client> {
    pub async fn recv(&mut self) -> Option<Option<(String, Publish)>> {
        self.rx.recv().await
    }

    pub fn clone_sender(&self) -> BridgeMessageSender {
        self.sender.clone()
    }

    fn new(
        target: Client,
        tx: mpsc::Sender<Option<(String, Publish)>>,
        rx: mpsc::Receiver<Option<(String, Publish)>>,
    ) -> Self {
        let (unbounded_tx, unbounded_rx) = mpsc::unbounded_channel();
        let companion_bridge_half = BridgeAsyncClient {
            target,
            rx,
            sender: BridgeMessageSender { unbounded_tx },
            published: Arc::new(AtomicUsize::new(0)),
            acknowledged: Arc::new(AtomicUsize::new(0)),
        };
        companion_bridge_half.spawn_publisher(tx, unbounded_rx);
        companion_bridge_half
    }

    fn publish(&mut self, target_topic: String, publish: Publish) {
        self.sender.publish(target_topic, publish)
    }

    fn ack(&mut self, publish: Publish) {
        self.sender.ack(publish)
    }

    fn published(&self) -> usize {
        self.published.load(Ordering::Relaxed)
    }

    fn acknowledged(&self) -> usize {
        self.acknowledged.load(Ordering::Relaxed)
    }

    fn spawn_publisher(
        &self,
        tx: mpsc::Sender<Option<(String, Publish)>>,
        mut unbounded_rx: mpsc::UnboundedReceiver<BridgeMessage>,
    ) {
        let target = self.target.clone();
        let published = self.published.clone();
        let acknowledged = self.acknowledged.clone();
        tokio::spawn(async move {
            while let Some(message) = unbounded_rx.recv().await {
                match message {
                    BridgeMessage::BridgePub {
                        target_topic,
                        publish,
                    } => {
                        let duplicate = (target_topic.clone(), publish.clone());
                        tx.send(Some(duplicate)).await.unwrap();
                        target
                            .publish(target_topic, publish.qos, publish.retain, publish.payload)
                            .await
                            .unwrap();
                        published.fetch_add(1, Ordering::Relaxed);
                    }
                    BridgeMessage::Pub { publish } => {
                        tx.send(None).await.unwrap();
                        target
                            .publish(publish.topic, publish.qos, publish.retain, publish.payload)
                            .await
                            .unwrap();
                    }
                    BridgeMessage::BridgeAck { publish } => {
                        target.ack(&publish).await.unwrap();
                        acknowledged.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
}

#[derive(Clone)]
struct BridgeMessageSender {
    unbounded_tx: mpsc::UnboundedSender<BridgeMessage>,
}

impl BridgeMessageSender {
    fn internal_publish(&mut self, publish: Publish) {
        self.unbounded_tx
            .send(BridgeMessage::Pub { publish })
            .unwrap()
    }

    fn publish(&mut self, target_topic: String, publish: Publish) {
        self.unbounded_tx
            .send(BridgeMessage::BridgePub {
                target_topic,
                publish,
            })
            .unwrap()
    }

    fn ack(&mut self, publish: Publish) {
        self.unbounded_tx
            .send(BridgeMessage::BridgeAck { publish })
            .unwrap()
    }
}

/// Forward messages received from `recv_event_loop` to `target`
///
/// The result of running this function constitutes half the MQTT bridge, hence the name.
/// Each half has two main responsibilities, one is to take messages received on the event loop and
/// forward them to the target client, the other is to communicate with the companion half of the
/// bridge to ensure published messages get acknowledged only when they have been fully processed.
///
/// # Message flow
/// Messages in the bridge go through a few states
/// 1. Received from the sending broker
/// 2. Forwarded to the receiving broker
/// 3. The receiving broker sends an acknowledgement
/// 4. The original forwarded message is acknowledged now we know it is fully processed
///
/// Since the function is processing one [EventLoop], messages can be sent to the receiving broker,
/// but we cannot receive acknowledgements from that broker, therefore a communication link must
/// be established between the bridge halves. This link is the argument `companion_bridge_half`,
/// which can both send and receive messages from the other bridge loop.
///
/// When a message is forwarded, the [Publish] is forwarded from this loop to the companion loop.
/// This allows the loop to store the message along with its packet ID when the forwarded message is
/// published. When an acknowledgement is received for the forwarded message, the packet id is used
/// to retrieve the original [Publish], which is then passed to [AsyncClient::ack] to complete the
/// final step of the message flow.
///
/// The channel sends [`Option<Publish>`] rather than [`Publish`] to allow the bridge to send entirely
/// novel messages, and not just forwarded ones, as attaching packet IDs relies on pairing every
/// [Outgoing] publish notification with a message sent by the relevant client. So, when a QoS 1
/// message is forwarded, this will be accompanied by sending `Some(message)` to the channel,
/// allowing the original message to be acknowledged once an acknowledgement is received for the
/// forwarded message. When publishing a health message, this will be accompanied by sending `None`
/// to the channel, telling the bridge to ignore the associated packet ID as this didn't arise from
/// a forwarded message that itself requires acknowledgement.
///
/// ## Bridging local messages to the cloud
///
/// The two `half-bridge` instances cooperate:
///
/// - The `half_bridge(local_event_loop,cloud_client)` receives local messages and publishes these message on the cloud.
/// - The `half_bridge(cloud_event_loop,local_client)` handles the acknowledgements: waiting for messages be acknowledged by the cloud, before sending acks for the original messages.
///
/// ```text
///                   ┌───────────────┐                                   ┌───────────────┐                                           
///                   │  (EventLoop)  │                                   │   (client)    │                                           
///  Incoming::PubAck │     ┌──┐      │                                   │     ┌──┐      │ client.ack                          
///  ─────────────────┼────►│6.├──────┼───────────────────┬───────────────┼────►│7.├──────┼──────────────►                            
///                   │     └──┘      │                   │               │     └──┘      │                                           
///                   │               │                   │               │               │                                           
///                   │               │             ┌─────┼────────┐      │               │                                           
///                   │               │             │     │        │      │               │                                           
/// Outgoing::Publish │     ┌──┐      │             │    ┌┴─┐      │      │               │
///  ─────────────────┼────►│4.├──────┼─────────────┼───►│5.│      │      │               │                                           
///                   │     └─▲┘      │             │    └▲─┘      │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │ half_bridge(cloud_event_loop,local_client)
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
/// xxxxxxxxxxxxxxxxxxxxxxxxxx│xxxxxxxxxxxxxxxxxxxxx│xxxxx│xxxxxxxx│xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │ half_bridge(local_event_loop,cloud_client)
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///                   │       │       │             │     │        │      │               │                                           
///    client.publish │      ┌┴─┐     │             │    ┌┴─┐      │      │     ┌──┐      │ Incoming::Publish                         
///  ◄────────────────┼──────┤3.◄─────┼─────────────┼────┤2.│◄─────┼──────┼─────┤1.│◄─────┼───────────────                            
///                   │      └──┘     │             │    └──┘      │      │     └──┘      │                                           
///                   │               │             │              │      │               │                                           
///                   │   MQTT        │             │              │      │    MQTT       │                                           
///                   │   cloud       │             └──────────────┘      │    local      │                                           
///                   │   connection  │                                   │    connection │                                           
///                   │               │                                   │               │                                           
///                   │   (client)    │                                   │  (EventLoop)  │                                           
///                   └───────────────┘                                   └───────────────┘                                           
/// ```
///
/// 1. A message is received via the local_event_loop.
/// 2. This message is sent unchanged to the second half_bridge which owns the local_client: so the latter will be able to acknowledge it when fully processed.
/// 3. A copy of this message is published by the cloud_client on the cloud topic derived from the local topic.
/// 4. The cloud_event_loop is notified that the message has been published by the cloud client. The notification event provides the `pkid` of the message used on the cloud connection.
/// 5. The message cloud `pkid` is joined with the original local message sent step 2. The pair (cloud `pkid`, local message) is cached
/// 6. The cloud MQTT end-point acknowledges the message, providing its cloud `pkid`.
/// 7. The pair (cloud `pkid`, local message) is extracted from the cache and the local message is finally acknowledged.
///  
/// ## Bridging cloud messages to the local broker
///
/// The very same two `half-bridge` instances ensure the reverse flow. Their roles are simply swapped:
///
/// - The `half_bridge(cloud_event_loop,local_client)` receives cloud messages and publishes these message locally.
/// - The `half_bridge(local_event_loop,cloud_client)` handles the acknowledgements: waiting for messages be acknowledged locally, before sending acks for the original messages.
///
/// # Health topics
/// The bridge will publish health information to `health_topic` (if supplied) on `target` to enable
/// other components to establish bridge health. This is intended to be used the half with cloud
/// event loop, so the status of this connection will be relayed to a relevant `te` topic like its
/// mosquitto-based predecessor. The payload is either `1` (healthy) or `0` (unhealthy). When the
/// connection is created, the last-will message is set to send the `0` payload when the connection
/// is dropped.
#[allow(clippy::too_many_arguments)]
async fn half_bridge(
    mut recv_event_loop: impl MqttEvents,
    recv_client: impl MqttClient + 'static,
    mut target: BridgeAsyncClient<impl MqttClient + 'static>,
    transformer: TopicConverter,
    bidirectional_topic_filters: Vec<Cow<'static, str>>,
    tx_health: mpsc::Sender<(&'static str, Status)>,
    name: &'static str,
    topics: Vec<SubscribeFilter>,
    reconnect_policy: TEdgeConfigReaderMqttBridgeReconnectPolicy,
) {
    let mut backoff = CustomBackoff::new(
        ::backoff::SystemClock {},
        reconnect_policy.initial_interval.duration(),
        reconnect_policy.maximum_interval.duration(),
        reconnect_policy.reset_window.duration(),
    );
    let mut forward_pkid_to_received_msg = HashMap::<u16, Publish>::new();
    let mut bridge_health = BridgeHealth::new(name, tx_health);
    let mut loop_breaker =
        MessageLoopBreaker::new(recv_client.clone(), bidirectional_topic_filters);

    let mut received = 0; // Count of messages received by this half-bridge
    let mut published = 0; // Count of messages published (by the companion)
    let mut acknowledged = 0; // Count of messages acknowledged (by the MQTT end-point of the companion)

    // Keeps track of whether we have a non-clean session with the broker. This
    // is set based on the value in the `ConnAck` packet to ensure it aligns
    // with whether a session exists, not just that we requested one. This is
    // used to republish messages in cases where rumqttc doesn't.
    let mut session_present: Option<bool> = None;
    let mut pending = Vec::new();

    loop {
        let res = recv_event_loop.poll().await;
        bridge_health.update(&res).await;

        let notification = match res {
            Ok(notification) => {
                backoff.mark_success();
                notification
            }
            Err(_) => {
                let time = backoff.backoff();
                if !time.is_zero() {
                    info!("Waiting {time:?} until attempting reconnection to {name} broker");
                }
                tokio::time::sleep(time).await;

                // If the session is not managed by the current connection,
                // handle the pending messages ourselves. If this isn't the
                // case, rumqttc will handle republishing messages as per
                // the MQTT specification.
                if session_present != Some(true) {
                    let msgs = recv_event_loop.take_pending();
                    debug!("Extending pending with: {msgs:?}");
                    pending.extend(msgs);
                }
                continue;
            }
        };
        debug!("Received notification ({name}) {notification:?}");
        debug!("Bridge {name} connection: received={received} forwarded={forwarded} published={published} waiting={waiting} acknowledged={acknowledged} finalized={finalized}",
            forwarded = target.published(),
            waiting = forward_pkid_to_received_msg.len(),
            finalized = target.acknowledged(),
        );

        match notification {
            Event::Incoming(Incoming::ConnAck(conn_ack)) => {
                info!("Bridge {name} connection subscribing to {topics:?}");

                let recv_client = recv_client.clone();
                let topics = topics.clone();
                // We have to subscribe to this asynchronously (i.e. in a task) since we might at
                // this point have filled our cloud event loop with outgoing messages
                tokio::spawn(async move { recv_client.subscribe_many(topics).await.unwrap() });

                session_present = Some(conn_ack.session_present);

                if !conn_ack.session_present {
                    // Republish any outstanding messages
                    let msgs = std::mem::take(&mut pending);
                    debug!("Setting pending messages to {msgs:?}");
                    recv_event_loop.set_pending(msgs);
                }
            }

            // Forward messages from event loop to target
            Event::Incoming(Incoming::Publish(publish)) => {
                if let Some(publish) = loop_breaker.ensure_not_looped(publish).await {
                    if let Some(topic) = transformer.convert_topic(&publish.topic) {
                        received += 1;
                        target.publish(topic.to_string(), publish);
                    } else {
                        // Being not forwarded to this bridge target
                        // The message has to be acknowledged
                        recv_client.ack(&publish).await.unwrap()
                    }
                }
            }

            // Forward acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                if let Some(msg) = forward_pkid_to_received_msg.remove(&ack_pkid) {
                    acknowledged += 1;
                    target.ack(msg);
                } else {
                    info!("Bridge {name} connection received ack for unknown pkid={ack_pkid}");
                }
            }

            // Keep track of packet IDs so we can acknowledge messages
            Event::Outgoing(Outgoing::Publish(pkid)) => {
                if let hash_map::Entry::Vacant(e) = forward_pkid_to_received_msg.entry(pkid) {
                    match target.recv().await {
                        // A message was forwarded by the other bridge half, note the packet id
                        Some(Some((topic, msg))) => {
                            published += 1;
                            loop_breaker.forward_on_topic(topic, &msg);
                            if pkid != 0 {
                                // Messages with pkid 0 (meaning QoS=0) should not be added to the hashmap
                                // as multiple messages with the pkid=0 can be received
                                e.insert(msg);
                            }
                        }

                        // A healthcheck message was published, ignore this packet id
                        Some(None) => {}

                        // The other bridge half has disconnected, break the loop and shut down the bridge
                        None => break,
                    }
                } else {
                    info!("Bridge {name} connection ignoring already known pkid={pkid}");
                }
            }

            Event::Outgoing(Outgoing::AwaitAck(pkid)) => {
                info!("Bridge {name} connection still waiting ack for pkid={pkid}");
            }

            Event::Incoming(Incoming::Disconnect) => {
                info!("Bridge {name} connection closed by peer");
            }

            _ => {}
        }
    }
}

#[async_trait::async_trait]
trait MqttEvents: Send {
    async fn poll(&mut self) -> Result<Event, ConnectionError>;
    fn take_pending(&mut self) -> VecDeque<Request>;
    fn set_pending(&mut self, requests: Vec<Request>);
}

#[async_trait::async_trait]
impl MqttEvents for EventLoop {
    async fn poll(&mut self) -> Result<Event, ConnectionError> {
        EventLoop::poll(self).await
    }

    fn take_pending(&mut self) -> VecDeque<Request> {
        std::mem::take(&mut self.pending)
    }

    fn set_pending(&mut self, requests: Vec<Request>) {
        self.pending = requests.into_iter().collect();
    }
}
#[async_trait::async_trait]
trait MqttClient: MqttAck + Clone + Send + Sync {
    async fn subscribe_many(&self, topics: Vec<SubscribeFilter>) -> Result<(), ClientError>;
    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Bytes,
    ) -> Result<(), ClientError>;
}

#[async_trait::async_trait]
impl MqttClient for AsyncClient {
    async fn subscribe_many(&self, topics: Vec<SubscribeFilter>) -> Result<(), ClientError> {
        AsyncClient::subscribe_many(self, topics).await
    }
    async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Bytes,
    ) -> Result<(), ClientError> {
        AsyncClient::publish(self, topic, qos, retain, payload).await
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Status {
    Up,
    Down,
}

impl Status {
    fn json(self) -> &'static str {
        match self {
            Status::Up => r#"{"status":"up"}"#,
            Status::Down => r#"{"status":"down"}"#,
        }
    }
}

fn overall_status(lhs: Option<Status>, rhs: &Option<Status>) -> Option<Status> {
    match (lhs?, rhs.as_ref()?) {
        (Status::Up, Status::Up) => Some(Status::Up),
        _ => Some(Status::Down),
    }
}

/// A tool to remove duplicate messages and avoid infinite loops
struct MessageLoopBreaker<Ack, Clock> {
    forwarded_messages: VecDeque<(Instant, Publish)>,
    bidirectional_topics: Vec<Cow<'static, str>>,
    client: Ack,
    clock: Clock,
}

fn have_same_content(fwd_msg: &Publish, cmp: &Publish) -> bool {
    fwd_msg.topic == cmp.topic
        && fwd_msg.qos == cmp.qos
        && fwd_msg.retain == cmp.retain
        && fwd_msg.payload == cmp.payload
}

#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
trait MqttAck {
    async fn ack(&self, publish: &Publish) -> Result<(), rumqttc::ClientError>;
}

#[async_trait::async_trait]
#[mutants::skip] // missed: replace <impl MqttAck for AsyncClient>::ack -> Result<(), ClientError> with Ok(())
impl MqttAck for AsyncClient {
    async fn ack(&self, publish: &Publish) -> Result<(), ClientError> {
        AsyncClient::ack(self, publish).await
    }
}

struct SystemClock;

#[cfg_attr(test, mockall::automock)]
trait MonotonicClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

impl MonotonicClock for SystemClock {}

impl<Ack: MqttAck> MessageLoopBreaker<Ack, SystemClock> {
    fn new(recv_client: Ack, bidirectional_topics: Vec<Cow<'static, str>>) -> Self {
        Self {
            forwarded_messages: <_>::default(),
            bidirectional_topics,
            client: recv_client,
            clock: SystemClock,
        }
    }
}

impl<Ack: MqttAck, Clock: MonotonicClock> MessageLoopBreaker<Ack, Clock> {
    async fn ensure_not_looped(&mut self, received: Publish) -> Option<Publish> {
        self.clean_old_messages();
        if self
            .forwarded_messages
            .front()
            .is_some_and(|(_, sent)| have_same_content(sent, &received))
        {
            self.client.ack(&received).await.unwrap();
            self.forwarded_messages.pop_front();
            None
        } else {
            Some(received)
        }
    }

    fn forward_on_topic(&mut self, topic: impl Into<String> + AsRef<str>, publish: &Publish) {
        if self.is_bidirectional(topic.as_ref()) {
            let mut publish_with_topic = Publish::new(topic, publish.qos, publish.payload.clone());
            publish_with_topic.retain = publish.retain;
            self.forwarded_messages
                .push_back((self.clock.now(), publish_with_topic));
        }
    }

    fn is_bidirectional(&self, topic: &str) -> bool {
        self.bidirectional_topics
            .iter()
            .any(|filter| matches_ignore_dollar_prefix(topic, filter))
    }

    #[mutants::skip]
    fn clean_old_messages(&mut self) {
        let deadline = self.clock.now() - Duration::from_secs(100);
        while self
            .forwarded_messages
            .front()
            .map(|&(time, _)| time < deadline)
            == Some(true)
        {
            self.forwarded_messages.pop_front();
        }
    }
}

impl Builder<MqttBridgeActor> for MqttBridgeActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<MqttBridgeActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> MqttBridgeActor {
        self.build_actor()
    }
}

impl RuntimeRequestSink for MqttBridgeActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        NullSender.into()
    }
}

pub struct MqttBridgeActor {}

#[async_trait]
impl Actor for MqttBridgeActor {
    #[mutants::skip]
    fn name(&self) -> &str {
        "MQTT-Bridge"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod message_loop_breaker {
        use crate::MessageLoopBreaker;
        use crate::MockMonotonicClock;
        use crate::MockMqttAck;
        use rumqttc::Publish;
        use rumqttc::QoS;
        use std::time::Duration;
        use std::time::Instant;

        #[tokio::test]
        async fn ignores_non_bidirectional_topics() {
            let client = MockMqttAck::new();
            let mut sut = MessageLoopBreaker::new(client, vec![]);
            let example_pub = Publish::new("test", QoS::AtMostOnce, "test");

            sut.forward_on_topic("test", &example_pub);
            assert_eq!(
                sut.ensure_not_looped(example_pub.clone()).await,
                Some(example_pub)
            );
        }

        #[tokio::test]
        async fn skips_forwarded_messages() {
            let mut client = MockMqttAck::new();
            let _ = client.expect_ack().return_once(|_| Ok(()));
            let mut sut = MessageLoopBreaker::new(client, vec!["test".into()]);
            let example_pub = Publish::new("test", QoS::AtMostOnce, "test");

            sut.forward_on_topic("test", &example_pub);
            assert_eq!(sut.ensure_not_looped(example_pub).await, None);
        }

        #[tokio::test]
        async fn allows_duplicate_messages_after_decloning() {
            let mut client = MockMqttAck::new();
            let _ack = client.expect_ack().return_once(|_| Ok(()));
            let mut sut = MessageLoopBreaker::new(client, vec!["test".into()]);
            let example_pub = Publish::new("test", QoS::AtMostOnce, "test");

            sut.forward_on_topic("test", &example_pub);
            sut.ensure_not_looped(example_pub.clone()).await;
            assert_eq!(
                sut.ensure_not_looped(example_pub.clone()).await,
                Some(example_pub)
            );
        }

        #[tokio::test]
        async fn deduplicates_topics_that_start_with_a_dollar_sign() {
            let mut client = MockMqttAck::new();
            let _ack = client.expect_ack().return_once(|_| Ok(()));
            let mut sut = MessageLoopBreaker::new(client, vec!["$aws/test".into()]);
            let example_pub = Publish::new("$aws/test", QoS::AtMostOnce, "test");

            sut.forward_on_topic("$aws/test", &example_pub);
            assert_eq!(sut.ensure_not_looped(example_pub.clone()).await, None);
        }

        #[tokio::test]
        async fn cleans_up_stale_messages() {
            let mut clock = MockMonotonicClock::new();
            let now = Instant::now();
            let _now = clock.expect_now().times(1).return_once(move || now);
            let mut sut = MessageLoopBreaker {
                client: MockMqttAck::new(),
                bidirectional_topics: vec!["test".into()],
                forwarded_messages: <_>::default(),
                clock,
            };
            let example_pub = Publish::new("test", QoS::AtMostOnce, "test");

            sut.forward_on_topic("test", &example_pub);
            let _now = sut
                .clock
                .expect_now()
                .times(1)
                .return_once(move || now + Duration::from_secs(3600));
            assert_eq!(
                sut.ensure_not_looped(example_pub.clone()).await,
                Some(example_pub)
            );
        }
    }

    mod topic_converter {
        use super::*;

        #[test]
        fn includes_local_rules_in_subscription_topics() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_local("s/us", "c8y/", "").unwrap();
            assert_eq!(tc.local_subscriptions().collect::<Vec<_>>(), ["c8y/s/us"]);
        }

        #[test]
        fn includes_remote_rules_in_subscription_topics() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            assert_eq!(tc.remote_subscriptions().collect::<Vec<_>>(), ["s/ds"]);
        }

        #[test]
        fn applies_local_rules_in_order() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_local("s/us", "c8y/", "").unwrap();
            tc.forward_from_local("#", "c8y/", "secondary/").unwrap();
            let [(rules, _), _] = tc.converters_and_bidirectional_topic_filters();
            assert_eq!(rules.convert_topic("c8y/s/us"), Some("s/us".into()));
            assert_eq!(
                rules.convert_topic("c8y/other"),
                Some("secondary/other".into())
            );
        }

        #[test]
        fn applies_remote_rules_in_order() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            tc.forward_from_remote("#", "c8y/", "secondary/").unwrap();
            let [_, (rules, _)] = tc.converters_and_bidirectional_topic_filters();
            assert_eq!(rules.convert_topic("s/ds"), Some("c8y/s/ds".into()));
            assert_eq!(
                rules.convert_topic("secondary/other"),
                Some("c8y/other".into())
            );
        }

        #[test]
        fn does_not_create_local_subscription_for_remote_rule() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            assert_eq!(tc.local_subscriptions().count(), 0);
        }

        #[test]
        fn does_not_create_remote_subscription_for_local_rule() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_local("s/us", "c8y/", "").unwrap();
            assert_eq!(tc.remote_subscriptions().count(), 0);
        }

        #[test]
        fn creates_multiple_subscriptions_when_multiple_local_rules_are_added() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_local("s/us", "c8y/", "").unwrap();
            tc.forward_from_local("s/uat", "c8y/", "").unwrap();
            assert_eq!(
                tc.local_subscriptions().collect::<Vec<_>>(),
                ["c8y/s/us", "c8y/s/uat"]
            );
        }

        #[test]
        fn creates_multiple_subscriptions_when_multiple_remote_rules_are_added() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            tc.forward_from_remote("s/dat", "c8y/", "").unwrap();
            assert_eq!(
                tc.remote_subscriptions().collect::<Vec<_>>(),
                ["s/ds", "s/dat"]
            );
        }

        #[test]
        fn creates_multiple_subscriptions_when_rules_are_added_in_both_directions() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_local("s/us", "c8y/", "").unwrap();
            tc.forward_from_local("s/uat", "c8y/", "").unwrap();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            tc.forward_from_remote("s/dat", "c8y/", "").unwrap();
            assert_eq!(
                tc.local_subscriptions().collect::<Vec<_>>(),
                ["c8y/s/us", "c8y/s/uat"]
            );
            assert_eq!(
                tc.remote_subscriptions().collect::<Vec<_>>(),
                ["s/ds", "s/dat"]
            );
        }
    }

    mod have_same_content {
        use crate::have_same_content;
        use rumqttc::Publish;
        use rumqttc::QoS;

        #[test]
        fn accepts_identical_messages() {
            let msg = Publish::new("test", QoS::AtLeastOnce, "a test");
            assert!(have_same_content(&msg, &msg));
        }

        #[test]
        fn rejects_messages_with_different_topics() {
            let msg = Publish::new("test", QoS::AtLeastOnce, "a test");
            let msg2 = Publish::new("not/the/same", QoS::AtLeastOnce, "a test");
            assert!(!have_same_content(&msg, &msg2));
        }
        #[test]
        fn rejects_messages_with_different_qos() {
            let msg = Publish::new("test", QoS::ExactlyOnce, "a test");
            let msg2 = Publish::new("test", QoS::AtLeastOnce, "a test");
            assert!(!have_same_content(&msg, &msg2));
        }

        #[test]
        fn rejects_messages_with_different_payloads() {
            let msg = Publish::new("test", QoS::ExactlyOnce, "a test");
            let msg2 = Publish::new("test", QoS::AtLeastOnce, "not the same");
            assert!(!have_same_content(&msg, &msg2));
        }

        #[test]
        fn rejects_messages_with_different_retain_values() {
            let mut msg = Publish::new("test", QoS::ExactlyOnce, "a test");
            msg.retain = true;
            let msg2 = Publish::new("test", QoS::AtLeastOnce, "a test");
            assert!(!have_same_content(&msg, &msg2));
        }
    }

    mod bridge {
        use std::time::Duration;

        use crate::test_helpers::*;
        use crate::*;
        use rumqttc::mqttbytes::v4::*;
        use rumqttc::Event;
        use rumqttc::QoS;
        use tedge_config::tedge_toml::TEdgeConfigReaderMqttBridgeReconnectPolicy;
        use tokio::sync::mpsc;
        use tokio::sync::mpsc::error::TryRecvError;
        use tokio::task::JoinHandle;

        #[tokio::test]
        async fn subscribes_after_conn_ack() {
            let events = [inc!(connack)];

            let subscription_topics = vec![SubscribeFilter::new(
                "subscription".into(),
                QoS::AtLeastOnce,
            )];
            let bridge = Bridge::default()
                .with_local_events(events)
                .with_subscription_topics(subscription_topics.clone())
                .process_all_events()
                .await;

            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::SubscribeMany(subscription_topics)
            )
        }

        #[tokio::test]
        async fn acknowledges_messages_that_arent_forwarded() {
            let msg = Publish::new("non-forwarded-topic", QoS::AtLeastOnce, "payload");
            let events = [inc!(publish(msg))];

            let bridge = Bridge::default()
                .with_local_events(events)
                .process_all_events()
                .await;

            assert_eq!(bridge.local_client.next_action().unwrap(), Action::Ack(msg))
        }

        #[tokio::test]
        async fn forwards_published_messages() {
            let incoming_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "payload");
            let outgoing_msg = Publish::new("s/us", QoS::AtLeastOnce, "payload");
            let events = [inc!(publish(incoming_msg))];

            let bridge = Bridge::default()
                .with_local_events(events)
                .with_c8y_topics()
                .process_all_events()
                .await;

            assert_eq!(
                bridge.cloud_client.next_action().unwrap(),
                Action::Publish(outgoing_msg)
            )
        }

        #[tokio::test]
        async fn forwards_message_acknowledgements() {
            let incoming_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "payload");
            let local_events = [inc!(publish(incoming_msg))];
            let cloud_events = [out!(publish(1)), inc!(puback(1))];

            let bridge = Bridge::default()
                .with_local_events(local_events)
                .with_cloud_events(cloud_events)
                .with_c8y_topics()
                .process_all_events()
                .await;

            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(incoming_msg)
            )
        }

        #[tokio::test]
        async fn subscribe_does_not_block_event_loop_polling() {
            // In the case where we connect to one broker immediately, and the
            // other broker's connection takes some time, the event loop may
            // fill up with pending messages. If the event loop is full, future
            // requests, such as `subscribe_many` will block until the event
            // loop is polled. This checks to see if a message is processed
            // while `subscribe_many` is blocking

            let publish = Publish::new("c8y/s/us", QoS::AtLeastOnce, "a message");
            let local_events = [
                // Send a connack to trigger a subscribe_many call
                inc!(connack),
                // And a publish so we can check future events are processed
                inc!(publish(publish)),
            ];
            let mut bridge = Bridge::default()
                .with_local_events(local_events)
                .with_local_client(BlockingSubscribeClient)
                .with_c8y_topics()
                .process_all_events()
                .await;
            bridge.assert_not_panicked().await;
            let expected_message = Publish {
                topic: "s/us".into(),
                ..publish
            };
            assert_eq!(
                bridge.cloud_client.next_action().unwrap(),
                Action::Publish(expected_message)
            )
        }

        #[tokio::test]
        async fn acknowledges_correct_messages_when_puback_out_of_order() {
            let first_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "first payload");
            let second_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "second payload");
            let local_events: [Result<Event, ConnectionError>; 2] =
                [inc!(publish(first_msg)), inc!(publish(second_msg))];
            let cloud_events = [
                out!(publish(1)),
                out!(publish(2)),
                inc!(puback(2)),
                inc!(puback(1)),
            ];

            let bridge = Bridge::default()
                .with_local_events(local_events)
                .with_cloud_events(cloud_events)
                .with_c8y_topics()
                .process_all_events()
                .await;

            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(second_msg)
            );
            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(first_msg)
            );
        }

        #[tokio::test]
        async fn acknowledges_messages_successfully_following_disconnection() {
            let first_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "first payload");
            let second_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "second payload");
            let local_events = [inc!(publish(first_msg)), inc!(publish(second_msg))];
            let cloud_events = [
                out!(publish(1)),
                // Abruptly disconnect client
                inc!(network_error),
                // Republish message after disconnect
                out!(publish(1)),
                inc!(puback(1)),
                // Then check we successfully acknowledge a future message with the same pkid
                out!(publish(1)),
                inc!(puback(1)),
            ];

            let bridge = Bridge::default()
                .with_local_events(local_events)
                .with_cloud_events(cloud_events)
                .with_c8y_topics()
                .process_all_events()
                .await;

            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(first_msg)
            );
            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(second_msg)
            );
        }

        #[tokio::test]
        async fn ignores_duplicate_acknowledgement_for_same_message() {
            let first_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "first payload");
            let second_msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "second payload");
            let local_events = [inc!(publish(first_msg)), inc!(publish(second_msg))];
            let cloud_events = [
                out!(publish(1)),
                inc!(puback(1)),
                // Simulate the cloud sending a second acknowledgement
                inc!(puback(1)),
                // Then publish the second message
                out!(publish(2)),
                inc!(puback(2)),
            ];

            let bridge = Bridge::default()
                .with_local_events(local_events)
                .with_cloud_events(cloud_events)
                .with_c8y_topics()
                .process_all_events()
                .await;

            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(first_msg)
            );
            assert_eq!(
                bridge.local_client.next_action().unwrap(),
                Action::Ack(second_msg)
            );
        }

        #[tokio::test]
        async fn health_success_is_not_published_before_client_connected() {
            let mut bridge = Bridge::default().process_all_events().await;
            // There should not be a health message since there are no events
            assert_eq!(bridge.next_health_message(), None);
        }

        #[tokio::test]
        async fn health_success_is_published_on_connack() {
            let mut bridge = Bridge::default()
                .with_local_events([inc!(connack)])
                .process_all_events()
                .await;
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Up)));
        }

        #[tokio::test]
        async fn health_success_is_only_published_on_status_change() {
            let mut bridge = Bridge::default()
                .with_local_events([inc!(connack), inc!(suback)])
                .process_all_events()
                .await;

            // Ignore the message from connack, we know that works due to previous test
            bridge.next_health_message();

            // The suback shouldn't generate another success message
            assert_eq!(bridge.next_health_message(), None)
        }

        #[tokio::test]
        async fn health_status_is_updated_on_error() {
            let mut bridge = Bridge::default()
                .with_local_events([inc!(connack), inc!(network_error)])
                .process_all_events()
                .await;
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Up)));
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Down)));
        }

        #[tokio::test]
        async fn health_errors_if_initial_connection_fails() {
            let mut bridge = Bridge::default()
                .with_local_events([inc!(network_error)])
                .process_all_events()
                .await;
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Down)));
        }

        #[tokio::test]
        async fn health_status_is_updated_following_recovery_from_error() {
            let mut bridge = Bridge::default()
                .with_local_events([inc!(network_error), inc!(connack)])
                .process_all_events()
                .await;
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Down)));
            assert_eq!(bridge.next_health_message(), Some(("local", Status::Up)));
        }

        #[tokio::test]
        async fn bridge_can_publish_many_messages() {
            let (cloud_client, cloud_events) = channel_client_and_events();

            let msg = Publish::new("c8y/s/us", QoS::AtLeastOnce, "generic msg");
            let message_count = 10000;
            let local_events: Vec<_> = std::iter::from_fn(|| Some(inc!(publish(&msg))))
                .take(message_count as usize)
                .collect();

            Bridge::default()
                .with_local_events(local_events)
                .with_cloud_client(cloud_client)
                .with_cloud_custom_events(cloud_events.clone())
                .with_c8y_topics()
                .process_all_events()
                .await;
            assert_eq!(cloud_events.message_count().await, message_count);
        }

        struct Bridge<LoEv, ClEv, LoCl, ClCl> {
            local_events: LoEv,
            cloud_events: ClEv,
            local_client: LoCl,
            cloud_client: ClCl,
            subscription_topics: Vec<SubscribeFilter>,
            local_topic_converter: TopicConverter,
            cloud_topic_converter: TopicConverter,
        }

        struct CompletedBridge<Local, Cloud> {
            local_client: Local,
            cloud_client: Cloud,
            local_task: Option<JoinHandle<()>>,
            cloud_task: Option<JoinHandle<()>>,
            rx_health: mpsc::Receiver<(&'static str, Status)>,
        }

        macro_rules! bridge_rule {
            ($base:literal -$remove:literal +$add:literal) => {
                BridgeRule::try_new($base.into(), $remove.into(), $add.into()).unwrap()
            };
        }

        impl Default for Bridge<FixedEventStream, FixedEventStream, ActionLogger, ActionLogger> {
            fn default() -> Self {
                Self {
                    local_events: <_>::default(),
                    cloud_events: <_>::default(),
                    local_client: <_>::default(),
                    cloud_client: <_>::default(),
                    subscription_topics: <_>::default(),
                    local_topic_converter: <_>::default(),
                    cloud_topic_converter: <_>::default(),
                }
            }
        }

        impl<LoEv, ClEv, LoCl, ClCl> Bridge<LoEv, ClEv, LoCl, ClCl>
        where
            LoEv: MqttEvents + AllProcessed + Clone + 'static,
            ClEv: MqttEvents + AllProcessed + Clone + 'static,
            LoCl: MqttClient + MqttAck + 'static + Clone,
            ClCl: MqttClient + MqttAck + 'static + Clone,
        {
            fn with_subscription_topics(self, topics: Vec<SubscribeFilter>) -> Self {
                Self {
                    subscription_topics: topics,
                    ..self
                }
            }

            fn with_c8y_topics(self) -> Self {
                let local_rules = vec![bridge_rule!("s/us" - "c8y/" + "")];
                let cloud_topic_converter =
                    TopicConverter(vec![bridge_rule!("s/ds" - "" + "c8y/")]);
                Self {
                    local_topic_converter: TopicConverter(local_rules),
                    cloud_topic_converter,
                    ..self
                }
            }

            fn with_local_client<C>(self, client: C) -> Bridge<LoEv, ClEv, C, ClCl> {
                Bridge {
                    local_client: client,
                    cloud_client: self.cloud_client,
                    local_events: self.local_events,
                    cloud_events: self.cloud_events,
                    subscription_topics: self.subscription_topics,
                    local_topic_converter: self.local_topic_converter,
                    cloud_topic_converter: self.cloud_topic_converter,
                }
            }

            fn with_cloud_client<C>(self, client: C) -> Bridge<LoEv, ClEv, LoCl, C> {
                Bridge {
                    cloud_client: client,
                    local_client: self.local_client,
                    local_events: self.local_events,
                    cloud_events: self.cloud_events,
                    subscription_topics: self.subscription_topics,
                    local_topic_converter: self.local_topic_converter,
                    cloud_topic_converter: self.cloud_topic_converter,
                }
            }

            fn with_cloud_custom_events<NewClEv>(
                self,
                events: NewClEv,
            ) -> Bridge<LoEv, NewClEv, LoCl, ClCl> {
                Bridge {
                    cloud_events: events,
                    local_events: self.local_events,
                    local_client: self.local_client,
                    cloud_client: self.cloud_client,
                    subscription_topics: self.subscription_topics,
                    local_topic_converter: self.local_topic_converter,
                    cloud_topic_converter: self.cloud_topic_converter,
                }
            }

            /// Spawn both bridge halves and wait for them to process all queued events
            async fn process_all_events(self) -> CompletedBridge<LoCl, ClCl> {
                let (tx0, rx0) = mpsc::channel(10);
                let (tx1, rx1) = mpsc::channel(10);

                let (tx_health, rx_health) = mpsc::channel(10);

                let local_task = tokio::spawn(half_bridge(
                    self.local_events.clone(),
                    self.local_client.clone(),
                    BridgeAsyncClient::new(self.cloud_client.clone(), tx0, rx1),
                    self.local_topic_converter,
                    vec![],
                    tx_health.clone(),
                    "local",
                    self.subscription_topics.clone(),
                    TEdgeConfigReaderMqttBridgeReconnectPolicy::test_value(),
                ));
                let cloud_task = tokio::spawn(half_bridge(
                    self.cloud_events.clone(),
                    self.cloud_client.clone(),
                    BridgeAsyncClient::new(self.local_client.clone(), tx1, rx0),
                    self.cloud_topic_converter,
                    vec![],
                    tx_health,
                    "cloud",
                    self.subscription_topics,
                    TEdgeConfigReaderMqttBridgeReconnectPolicy::test_value(),
                ));

                tokio::time::timeout(Duration::from_secs(5), self.local_events.all_processed())
                    .await
                    .expect("Expected all local events to finish processing")
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(5), self.cloud_events.all_processed())
                    .await
                    .expect("Expected all cloud events to finish processing")
                    .unwrap();
                CompletedBridge {
                    local_client: self.local_client,
                    cloud_client: self.cloud_client,
                    local_task: Some(local_task),
                    cloud_task: Some(cloud_task),
                    rx_health,
                }
            }
        }

        impl<Ev, LoCl, ClCl> Bridge<FixedEventStream, Ev, LoCl, ClCl> {
            fn with_local_events<E>(self, events: E) -> Self
            where
                FixedEventStream: From<E>,
            {
                Self {
                    local_events: FixedEventStream::from(events),
                    ..self
                }
            }
        }

        impl<Ev, LoCl, ClCl> Bridge<Ev, FixedEventStream, LoCl, ClCl> {
            fn with_cloud_events<E>(self, events: E) -> Self
            where
                FixedEventStream: From<E>,
            {
                Self {
                    cloud_events: FixedEventStream::from(events),
                    ..self
                }
            }
        }

        impl<Local, Cloud> CompletedBridge<Local, Cloud> {
            pub async fn assert_not_panicked(&mut self) {
                if self.local_task.as_ref().map(|b| b.is_finished()) == Some(true) {
                    self.local_task.take().unwrap().await.unwrap();
                }
                if self.cloud_task.as_ref().map(|b| b.is_finished()) == Some(true) {
                    self.cloud_task.take().unwrap().await.unwrap();
                }
            }

            pub fn next_health_message(&mut self) -> Option<(&'static str, Status)> {
                match self.rx_health.try_recv() {
                    Ok(value) => Some(value),
                    Err(TryRecvError::Disconnected) => {
                        panic!("`tx_health` dropped unexpectedly, did the bridge crash?")
                    }
                    Err(TryRecvError::Empty) => None,
                }
            }
        }
    }
}
