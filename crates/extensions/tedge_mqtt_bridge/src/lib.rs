use async_trait::async_trait;
use certificate::parse_root_certificate::create_tls_config;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::matches;
use rumqttc::valid_filter;
use rumqttc::valid_topic;
use rumqttc::AsyncClient;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::Incoming;
use rumqttc::LastWill;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::PubAck;
use rumqttc::PubRec;
use rumqttc::Publish;
use rumqttc::Transport;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::NullSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tracing::error;
use tracing::log::info;

pub type MqttConfig = mqtt_channel::Config;

pub use mqtt_channel::DebugPayload;
pub use mqtt_channel::MqttError;
pub use mqtt_channel::MqttMessage;
pub use mqtt_channel::QoS;
pub use mqtt_channel::Topic;
use tedge_api::main_device_health_topic;
use tedge_config::MqttAuthConfig;
use tedge_config::TEdgeConfig;

#[derive(Default, Debug, Clone)]
pub struct BridgeConfig {
    local_to_remote: Vec<BridgeRule>,
    remote_to_local: Vec<BridgeRule>,
}

#[derive(Debug, Clone)]
pub struct BridgeRule {
    topic_filter: Cow<'static, str>,
    prefix_to_remove: Cow<'static, str>,
    prefix_to_add: Cow<'static, str>,
}

fn prepend<'a>(target: Cow<'a, str>, prefix: &Cow<'a, str>) -> Cow<'a, str> {
    match (prefix, target) {
        (prefix, target) if prefix.is_empty() => target,
        (prefix, target) if target.is_empty() => prefix.clone(),
        (prefix, Cow::Borrowed(target)) => format!("{prefix}{target}").into(),
        (prefix, Cow::Owned(mut target)) => {
            target.insert_str(0, prefix.as_ref());
            Cow::Owned(target)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidBridgeRule {
    #[error("{0:?} is not a valid MQTT bridge topic prefix as is missing a trailing slash")]
    MissingTrailingSlash(Cow<'static, str>),

    #[error(
        "{0:?} is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"
    )]
    Empty(BridgeRule),

    #[error("{0:?} is not a valid MQTT bridge topic prefix because it contains '+' or '#'")]
    InvalidTopicPrefix(String),

    #[error("{0:?} is not a valid MQTT bridge topic filter")]
    InvalidTopicFilter(String),
}

fn validate_topic(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_topic(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicPrefix(topic.to_owned())),
    }
}

fn validate_filter(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_filter(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicFilter(topic.to_owned())),
    }
}

impl BridgeRule {
    fn try_new(
        base_topic_filter: Cow<'static, str>,
        prefix_to_remove: Cow<'static, str>,
        prefix_to_add: Cow<'static, str>,
    ) -> Result<Self, InvalidBridgeRule> {
        let filter_is_empty = base_topic_filter.is_empty();
        let mut r = Self {
            topic_filter: prepend(base_topic_filter.clone(), &prefix_to_remove),
            prefix_to_remove,
            prefix_to_add,
        };

        validate_topic(&r.prefix_to_add)?;
        validate_topic(&r.prefix_to_remove)?;
        if filter_is_empty {
            if r.prefix_to_add.is_empty() || r.prefix_to_remove.is_empty() {
                r.topic_filter = base_topic_filter;
                Err(InvalidBridgeRule::Empty(r))
            } else {
                Ok(r)
            }
        } else if !(r.prefix_to_remove.ends_with('/') || r.prefix_to_remove.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_remove))
        } else if !(r.prefix_to_add.ends_with('/') || r.prefix_to_add.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_add))
        } else {
            validate_filter(&base_topic_filter)?;
            Ok(r)
        }
    }

    fn apply<'a>(&self, topic: &'a str) -> Option<Cow<'a, str>> {
        matches(topic, &self.topic_filter).then(|| {
            prepend(
                topic.strip_prefix(&*self.prefix_to_remove).unwrap().into(),
                &self.prefix_to_add,
            )
        })
    }
}

impl BridgeConfig {
    pub fn new() -> Self {
        Self::default()
    }

    // TODO forward bidirectional?
    pub fn forward_from_local(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.local_to_remote.push(BridgeRule::try_new(
            topic.into(),
            local_prefix.into(),
            remote_prefix.into(),
        )?);
        Ok(())
    }

    pub fn forward_from_remote(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.remote_to_local.push(BridgeRule::try_new(
            topic.into(),
            remote_prefix.into(),
            local_prefix.into(),
        )?);
        Ok(())
    }

    pub fn local_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.local_to_remote
            .iter()
            .map(|rule| rule.topic_filter.as_ref())
    }

    pub fn remote_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.remote_to_local.iter().map(|rule| &*rule.topic_filter)
    }

    // TODO local and remote could get confused here
    fn converters(self) -> [TopicConverter; 2] {
        let Self {
            local_to_remote,
            remote_to_local,
        } = self;

        [
            TopicConverter(local_to_remote),
            TopicConverter(remote_to_local),
        ]
    }
}

struct TopicConverter(Vec<BridgeRule>);

impl TopicConverter {
    fn convert_topic<'a>(&'a self, topic: &'a str) -> Cow<'a, str> {
        self.0.iter().find_map(|rule| rule.apply(topic)).unwrap()
    }
}

pub struct MqttBridgeActorBuilder {}

impl MqttBridgeActorBuilder {
    pub async fn new(
        tedge_config: &TEdgeConfig,
        service_name: String,
        rules: BridgeConfig,
        root_cert_path: impl AsRef<Path>,
    ) -> Self {
        let tls_config = create_tls_config(
            &root_cert_path,
            &tedge_config.device.key_path,
            &tedge_config.device.cert_path,
        )
        .unwrap();

        let mut local_config = MqttOptions::new(
            &service_name,
            &tedge_config.mqtt.client.host,
            tedge_config.mqtt.client.port.into(),
        );
        let health_topic = main_device_health_topic(&service_name);
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
        local_config.set_manual_acks(true);
        local_config.set_last_will(LastWill::new(
            &health_topic,
            BridgeHealth::DOWN_PAYLOAD,
            QoS::AtLeastOnce,
            true,
        ));
        local_config.set_clean_session(false);
        let mut cloud_config = MqttOptions::new(
            tedge_config.device.id.try_read(tedge_config).unwrap(),
            tedge_config
                .c8y
                .mqtt
                .or_config_not_set()
                .unwrap()
                .host()
                .to_string(),
            8883,
        );
        cloud_config.set_manual_acks(true);
        // Cumulocity tells us not to use clean session, so don't
        // https://cumulocity.com/docs/device-integration/mqtt/#mqtt-clean-session
        cloud_config.set_clean_session(true);

        cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));
        let (local_client, local_event_loop) = AsyncClient::new(local_config, 10);
        let (cloud_client, cloud_event_loop) = AsyncClient::new(cloud_config, 10);

        for topic in rules.local_subscriptions() {
            local_client
                .subscribe(topic, QoS::AtLeastOnce)
                .await
                .unwrap();
        }
        for topic in rules.remote_subscriptions() {
            cloud_client
                .subscribe(topic, QoS::AtLeastOnce)
                .await
                .unwrap();
        }

        let [msgs_local, msgs_cloud] = bidirectional_channel(10);
        let [convert_local, convert_cloud] = rules.converters();
        tokio::spawn(half_bridge(
            local_event_loop,
            cloud_client,
            convert_local,
            msgs_local,
            None,
            "local",
        ));
        tokio::spawn(half_bridge(
            cloud_event_loop,
            local_client,
            convert_cloud,
            msgs_cloud,
            Some(health_topic),
            "cloud",
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
        BidirectionalChannelHalf {
            tx: tx_first,
            rx: rx_second,
        },
        BidirectionalChannelHalf {
            tx: tx_second,
            rx: rx_first,
        },
    ]
}

struct BidirectionalChannelHalf<T> {
    tx: mpsc::Sender<T>,
    rx: mpsc::Receiver<T>,
}

impl<'a, T> BidirectionalChannelHalf<T> {
    pub fn send(&'a mut self, item: T) -> futures::sink::Send<'a, mpsc::Sender<T>, T> {
        self.tx.send(item)
    }

    pub fn recv(&mut self) -> futures::stream::Next<mpsc::Receiver<T>> {
        self.rx.next()
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
async fn half_bridge(
    mut recv_event_loop: EventLoop,
    target: AsyncClient,
    transformer: TopicConverter,
    mut companion_bridge_half: BidirectionalChannelHalf<Option<Publish>>,
    health_topic: Option<String>,
    name: &'static str,
) {
    let mut forward_pkid_to_received_msg = HashMap::new();
    let mut bridge_health = BridgeHealth::new(name, health_topic, &target);

    loop {
        let res = recv_event_loop.poll().await;
        bridge_health.update(&res, &mut companion_bridge_half).await;

        let notification = match res {
            Ok(notification) => notification,
            Err(_) => continue,
        };

        match notification {
            // Forward messages from event loop to target
            Event::Incoming(Incoming::Publish(publish)) => {
                target
                    .publish(
                        transformer.convert_topic(&publish.topic),
                        publish.qos,
                        publish.retain,
                        publish.payload.clone(),
                    )
                    .await
                    .unwrap();
                let publish = (publish.qos > QoS::AtMostOnce).then_some(publish);
                companion_bridge_half.send(publish).await.unwrap();
            }

            // Forward acks from event loop to target
            Event::Incoming(
                Incoming::PubAck(PubAck { pkid: ack_pkid })
                | Incoming::PubRec(PubRec { pkid: ack_pkid }),
            ) => {
                if let Some(msg) = forward_pkid_to_received_msg.remove(&ack_pkid) {
                    target.ack(&msg).await.unwrap();
                }
            }

            // Keep track of packet IDs so we can acknowledge messages
            Event::Outgoing(Outgoing::Publish(pkid)) => match companion_bridge_half.recv().await {
                // A message was forwarded by the other bridge half, note the packet id
                Some(Some(msg)) => {
                    forward_pkid_to_received_msg.insert(pkid, msg);
                }

                // A healthcheck message was published, ignore this packet id
                Some(None) => {}

                // The other bridge half has disconnected, break the loop and shut down the bridge
                None => break,
            },
            _ => {}
        }
    }
}

type NotificationRes = Result<Event, ConnectionError>;

struct BridgeHealth<'a> {
    name: &'static str,
    health_topic: Option<String>,
    target: &'a AsyncClient,
    last_err: Option<String>,
}

impl<'a> BridgeHealth<'a> {
    const UP_PAYLOAD: &'static str = "{\"status\":\"up\"}";
    const DOWN_PAYLOAD: &'static str = "{\"status\":\"down\"}";

    fn new(name: &'static str, health_topic: Option<String>, target: &'a AsyncClient) -> Self {
        Self {
            name,
            health_topic,
            target,
            last_err: Some("dummy error".into()),
        }
    }

    async fn update(
        &mut self,
        result: &NotificationRes,
        companion_bridge_half: &mut BidirectionalChannelHalf<Option<Publish>>,
    ) {
        let name = self.name;
        let (err, health_payload) = match result {
            Ok(event) => {
                if let Event::Incoming(Incoming::ConnAck(_)) = event {
                    info!("MQTT bridge connected to {name} broker")
                }
                (None, Self::UP_PAYLOAD)
            }
            Err(err) => (Some(err.to_string()), Self::DOWN_PAYLOAD),
        };

        if self.last_err != err {
            if let Some(err) = &err {
                error!("MQTT bridge failed to connect to {name} broker: {err}")
            }
            self.last_err = err;

            if let Some(health_topic) = &self.health_topic {
                self.target
                    .publish(health_topic, QoS::AtLeastOnce, true, health_payload)
                    .await
                    .unwrap();
                // Send a note that a message has been published to maintain synchronisation
                // between the two bridge halves
                companion_bridge_half.send(None).await.unwrap();
            }
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
            let [rules, _] = tc.converters();
            assert_eq!(rules.convert_topic("c8y/s/us"), "s/us");
            assert_eq!(rules.convert_topic("c8y/other"), "secondary/other");
        }

        #[test]
        fn applies_remote_rules_in_order() {
            let mut tc = BridgeConfig::new();
            tc.forward_from_remote("s/ds", "c8y/", "").unwrap();
            tc.forward_from_remote("#", "c8y/", "secondary/").unwrap();
            let [_, rules] = tc.converters();
            assert_eq!(rules.convert_topic("s/ds"), "c8y/s/ds");
            assert_eq!(rules.convert_topic("secondary/other"), "c8y/other");
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

    mod bridge_rule {
        use super::*;

        #[test]
        fn forward_topics_without_any_prefixes() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()))
        }

        #[test]
        fn forwards_wildcard_topics() {
            let rule = BridgeRule::try_new("a/#".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()));
        }

        #[test]
        fn does_not_forward_non_matching_topics() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("different/topic"), None)
        }

        #[test]
        fn removes_local_prefix() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("topic".into()));
        }

        #[test]
        fn prepends_remote_prefix() {
            // TODO maybe debug warn if topic filter begins with prefix to remove
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("b/topic".into()));
        }

        #[test]
        fn does_not_clone_if_topic_is_unchanged() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn does_not_clone_if_prefix_is_removed_but_not_added() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn forwards_unfiltered_topic() {
            let cloud_topic = "thinedge/devices/my-device/test-connection";
            let rule =
                BridgeRule::try_new("".into(), "aws/test-connection".into(), cloud_topic.into())
                    .unwrap();
            assert_eq!(rule.apply("aws/test-connection"), Some(cloud_topic.into()))
        }

        #[test]
        fn allows_empty_input_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "".into(), "output/".into()).unwrap();
            assert_eq!(rule.apply("test/me"), Some("output/test/me".into()));
        }

        #[test]
        fn allows_empty_output_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "input/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("input/test/me"), Some("test/me".into()));
        }

        #[test]
        fn rejects_invalid_input_topic() {
            let err = BridgeRule::try_new("test/#".into(), "wildcard/#".into(), "output/".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/#\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_invalid_output_topic() {
            let err = BridgeRule::try_new("test/#".into(), "input/".into(), "wildcard/+".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/+\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_input_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input".into(), "output/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"input\" is not a valid MQTT bridge topic prefix as is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_output_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input/".into(), "output".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"output\" is not a valid MQTT bridge topic prefix as is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_empty_input_topic_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "".into(), "a/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "", prefix_to_add: "a/" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }

        #[test]
        fn rejects_empty_output_topic_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "a/".into(), "".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "a/", prefix_to_add: "" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }
    }

    mod prepend {
        use super::*;

        #[test]
        fn applies_nonempty_prefix_to_start_of_value() {
            assert_eq!(prepend("tested".into(), &"being ".into()), "being tested");
        }

        #[test]
        fn does_not_clone_if_prefix_is_empty() {
            assert!(matches!(
                prepend("test".into(), &"".into()),
                Cow::Borrowed(_)
            ));
        }

        #[test]
        fn leaves_value_unchanged_if_prefix_is_empty() {
            assert_eq!(prepend("test".into(), &"".into()), "test");
        }
    }

    mod single_converter {
        use super::*;
        #[test]
        fn applies_matching_topic() {
            let converter = TopicConverter(vec![BridgeRule::try_new(
                "topic".into(),
                "a/".into(),
                "b/".into(),
            )
            .unwrap()]);
            assert_eq!(converter.convert_topic("a/topic"), "b/topic")
        }

        #[test]
        fn applies_first_matching_topic_if_multiple_are_provided() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), "b/topic");
        }

        #[test]
        fn does_not_apply_non_matching_topics() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "x/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), "c/topic");
        }
    }
}
