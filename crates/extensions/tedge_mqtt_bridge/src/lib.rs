mod backoff;
mod config;
mod health;
mod topics;

use async_trait::async_trait;
use certificate::parse_root_certificate::create_tls_config;
use futures::SinkExt;
use futures::StreamExt;
pub use rumqttc;
use rumqttc::AsyncClient;
use rumqttc::ClientError;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::LastWill;
pub use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::PubAck;
use rumqttc::PubRec;
use rumqttc::Publish;
use rumqttc::SubscribeFilter;
use rumqttc::Transport;
use std::borrow::Cow;
use std::collections::hash_map;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::NullSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
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
use tedge_config::MqttAuthConfig;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigReaderMqttBridgeReconnectPolicy;

use crate::backoff::CustomBackoff;
use crate::topics::matches_ignore_dollar_prefix;
use crate::topics::TopicConverter;
pub use config::*;

const MAX_PACKET_SIZE: usize = 268435455; // maximum allowed MQTT payload size

pub struct MqttBridgeActorBuilder {}

impl MqttBridgeActorBuilder {
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
        let local_tls_config = match auth_config {
            MqttAuthConfig {
                ca_dir: Some(ca_dir),
                client: Some(client),
                ..
            } => Some(create_tls_config(ca_dir, &client.key_file, &client.cert_file).unwrap()),
            MqttAuthConfig {
                ca_file: Some(ca_file),
                client: Some(client),
                ..
            } => Some(create_tls_config(ca_file, &client.key_file, &client.cert_file).unwrap()),
            _ => None,
        };
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

        let [msgs_local, msgs_cloud] = bidirectional_channel(in_flight.into());
        let [(convert_local, bidir_local), (convert_cloud, bidir_cloud)] =
            rules.converters_and_bidirectional_topic_filters();
        let (tx_status, monitor) = BridgeHealthMonitor::new(health_topic.name.clone(), &msgs_cloud);
        tokio::spawn(monitor.monitor());
        tokio::spawn(half_bridge(
            local_event_loop,
            cloud_client.clone(),
            local_client.clone(),
            convert_local,
            bidir_local,
            msgs_local,
            tx_status.clone(),
            "local",
            local_topics,
            reconnect_policy.clone(),
        ));
        tokio::spawn(half_bridge(
            cloud_event_loop,
            local_client.clone(),
            cloud_client.clone(),
            convert_cloud,
            bidir_cloud,
            msgs_cloud,
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

fn bidirectional_channel<T>(buffer: usize) -> [BidirectionalChannelHalf<T>; 2] {
    let (tx_first, rx_first) = mpsc::channel(buffer);
    let (tx_second, rx_second) = mpsc::channel(buffer);
    [
        BidirectionalChannelHalf::new(tx_first, rx_second),
        BidirectionalChannelHalf::new(tx_second, rx_first),
    ]
}

struct BidirectionalChannelHalf<T> {
    /// Sends messages to the companion half bridge
    tx: mpsc::Sender<Option<(String, T)>>,
    /// Receives messages from the companion half bridge
    rx: mpsc::Receiver<Option<(String, T)>>,
    /// Sends to a background task that forwards the messages to the target and companion
    ///
    /// (None, message) => {
    ///     - the message is published unchanged to the target
    ///     - a None sentinel value is sent to the companion
    /// }
    /// (Some(topic), message) => {
    ///     - the message is published to the target on the given topic
    ///     - Some(topic, message.clone()) is sent to the companion
    /// }
    ///
    unbounded_tx: mpsc::UnboundedSender<(Option<String>, T)>,
    /// Used by the background task
    unbounded_rx: Option<mpsc::UnboundedReceiver<(Option<String>, T)>>,
}

impl<'a, T> BidirectionalChannelHalf<T> {
    fn new(tx: mpsc::Sender<Option<(String, T)>>, rx: mpsc::Receiver<Option<(String, T)>>) -> Self {
        let (unbounded_tx, unbounded_rx) = mpsc::unbounded::<(Option<String>, T)>();
        BidirectionalChannelHalf {
            tx,
            rx,
            unbounded_tx,
            unbounded_rx: Some(unbounded_rx),
        }
    }

    pub fn send(
        &'a mut self,
        target_topic: Option<String>,
        message: T,
    ) -> futures::sink::Send<'a, mpsc::UnboundedSender<(Option<String>, T)>, (Option<String>, T)>
    {
        self.unbounded_tx.send((target_topic, message))
    }

    pub fn recv(&mut self) -> futures::stream::Next<mpsc::Receiver<Option<(String, T)>>> {
        self.rx.next()
    }

    pub fn clone_sender(&self) -> mpsc::UnboundedSender<(Option<String>, T)> {
        self.unbounded_tx.clone()
    }
}

impl BidirectionalChannelHalf<Publish> {
    pub fn spawn_publisher(&mut self, target: AsyncClient) {
        let mut unbounded_rx = self.unbounded_rx.take().unwrap();
        let mut tx = self.tx.clone();
        tokio::spawn(async move {
            while let Some((target_topic, publish)) = unbounded_rx.next().await {
                let topic = target_topic.clone().unwrap_or(publish.topic.clone());
                let duplicate = target_topic.map(|topic| (topic, publish.clone()));
                tx.send(duplicate).await.unwrap();
                target
                    .publish(topic, publish.qos, publish.retain, publish.payload)
                    .await
                    .unwrap();
            }
        });
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
    mut recv_event_loop: EventLoop,
    target: AsyncClient,
    recv_client: AsyncClient,
    transformer: TopicConverter,
    bidirectional_topic_filters: Vec<Cow<'static, str>>,
    mut companion_bridge_half: BidirectionalChannelHalf<Publish>,
    tx_health: mpsc::Sender<(&'static str, Status)>,
    name: &'static str,
    topics: Vec<SubscribeFilter>,
    reconnect_policy: TEdgeConfigReaderMqttBridgeReconnectPolicy,
) {
    companion_bridge_half.spawn_publisher(target.clone());

    let mut backoff = CustomBackoff::new(
        ::backoff::SystemClock {},
        reconnect_policy.initial_interval.duration(),
        reconnect_policy.maximum_interval.duration(),
        reconnect_policy.reset_window.duration(),
    );
    let mut forward_pkid_to_received_msg = HashMap::new();
    let mut bridge_health = BridgeHealth::new(name, tx_health);
    let mut loop_breaker =
        MessageLoopBreaker::new(recv_client.clone(), bidirectional_topic_filters);

    let mut received = 0; // Count of messages received by this half-bridge
    let mut forwarded = 0; // Count of messages forwarded to the companion half-bridge
    let mut published = 0; // Count of messages published (by the companion)
    let mut acknowledged = 0; // Count of messages acknowledged (by the MQTT end-point of the companion)
    let mut finalized = 0; // Count of messages fully processed by this half-bridge
    let mut ignored = 0; // Count of messages published to soon by the companion (AwaitAck)

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
                continue;
            }
        };
        debug!("Received notification ({name}) {notification:?}");
        debug!("Bridge {name} connection: received={received} forwarded={forwarded} published={published} waiting={} acknowledged={acknowledged} finalized={finalized} ignored={ignored}",
            forward_pkid_to_received_msg.len()
        );

        match notification {
            Event::Incoming(Incoming::ConnAck(_)) => {
                info!("Bridge {name} connection subscribing to {topics:?}");
                let recv_client = recv_client.clone();
                let topics = topics.clone();
                // We have to subscribe to this asynchronously (i.e. in a task) since we might at
                // this point have filled our cloud event loop with outgoing messages
                tokio::spawn(async move { recv_client.subscribe_many(topics).await.unwrap() });
            }

            // Forward messages from event loop to target
            Event::Incoming(Incoming::Publish(publish)) => {
                received += 1;
                if let Some(publish) = loop_breaker.ensure_not_looped(publish).await {
                    if let Some(topic) = transformer.convert_topic(&publish.topic) {
                        companion_bridge_half
                            .send(Some(topic.to_string()), publish)
                            .await
                            .unwrap();
                        forwarded += 1;
                    }
                }
            }

            // Forward acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                acknowledged += 1;
                if let Some(msg) = forward_pkid_to_received_msg.remove(&ack_pkid) {
                    if let Err(err) = target.ack(&msg).await {
                        info!("Bridge {name} connection failed to ack: {err:?}");
                    } else {
                        finalized += 1;
                    }
                } else {
                    info!("Bridge {name} connection received ack for unknown pkid={ack_pkid}");
                }
            }

            // Keep track of packet IDs so we can acknowledge messages
            Event::Outgoing(Outgoing::Publish(pkid)) => {
                published += 1;
                if let hash_map::Entry::Vacant(e) = forward_pkid_to_received_msg.entry(pkid) {
                    match companion_bridge_half.recv().await {
                        // A message was forwarded by the other bridge half, note the packet id
                        Some(Some((topic, msg))) => {
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
                    ignored += 1;
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
            .map_or(false, |(_, sent)| have_same_content(sent, &received))
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
            let client = MockMqttAck::new();
            let mut clock = MockMonotonicClock::new();
            let now = Instant::now();
            let _now = clock.expect_now().times(1).return_once(move || now);
            let mut sut = MessageLoopBreaker {
                client,
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
}
