//! Tracks whether the bridge to the cloud can relay the messages published to it
//!
//! On a fresh session the bridge holds no subscription on the local broker, so anything published
//! to the cloud topics before it does is discarded rather than forwarded. The mapper holds back its
//! own connection to the broker until then, so that no actor sharing it publishes too early.
//!
//! The bridge either runs in this process, where it signals directly once it holds the
//! subscriptions it relays, or runs in the broker, where the only sign of it is the health message
//! it publishes. Either way the wait happens once, not on every reconnection: the bridge keeps its
//! session on the local broker, so from then on the broker queues what it cannot deliver instead of
//! discarding it.

use crate::config::C8yMapperConfig;
use crate::service_monitor::is_c8y_bridge_established;
use async_trait::async_trait;
use std::convert::Infallible;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::GateConnection;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tokio::sync::watch;

/// Bounds how long the mapper holds back its connection to the broker waiting for the bridge
///
/// That wait is normally sub-second, so reaching this limit means the bridge cannot be seen at all —
/// its configuration is missing from the broker, say. A mapper that never connects is worse than one
/// whose first cloud-bound messages may be discarded, so it carries on regardless.
pub const MAX_BRIDGE_WAIT: Duration = Duration::from_secs(30);

/// Watches the bridge on behalf of the actors publishing cloud-bound messages of their own
pub struct BridgeMonitorBuilder {
    source: BridgeSource,

    /// Carries the health messages of a bridge running in the broker, and the shutdown request
    /// whichever bridge is monitored
    messages: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,

    ready: watch::Sender<bool>,
}

/// How the bridge being monitored makes itself known
enum BridgeSource {
    /// The built-in bridge signals when it holds the subscriptions it relays
    BuiltIn(watch::Receiver<bool>),

    /// A bridge running in the broker is only known by the health message it publishes
    InBroker {
        mqtt_schema: MqttSchema,
        health_topic: Topic,
    },
}

impl BridgeMonitorBuilder {
    /// Monitors the built-in bridge running in this process, on behalf of `connection`
    ///
    /// `subscribed` is the signal that bridge raises once it holds the subscriptions relaying the
    /// cloud topics, as the broker discards anything published to them before then
    pub fn built_in(
        subscribed: watch::Receiver<bool>,
        connection: &mut impl GateConnection,
    ) -> Self {
        Self::with_source(BridgeSource::BuiltIn(subscribed), connection)
    }

    /// Monitors a bridge running in the broker, through the health topic it publishes to
    ///
    /// `service_monitor` is a dedicated MQTT client, as the bridge has to be seen before anything
    /// is published through `connection`, the one the mapper shares with the other actors
    pub fn in_broker(
        service_monitor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        config: &C8yMapperConfig,
        connection: &mut impl GateConnection,
    ) -> Self {
        let builder = Self::with_source(
            BridgeSource::InBroker {
                mqtt_schema: config.mqtt_schema.clone(),
                health_topic: config.bridge_health_topic.clone(),
            },
            connection,
        );
        service_monitor.connect_sink(config.bridge_health_topic.clone().into(), &builder.messages);
        builder
    }

    /// Holds `connection` back until the bridge can relay what is published through it
    ///
    /// The readiness handed over only ever goes from `false` to `true`. A sender dropped while
    /// still `false` means the bridge was never seen and nothing is left to see it, which releases
    /// the connection rather than leaving it closed for good.
    fn with_source(source: BridgeSource, connection: &mut impl GateConnection) -> Self {
        let (ready, _) = watch::channel(false);
        connection.connect_when(ready.subscribe(), MAX_BRIDGE_WAIT);
        Self {
            source,
            messages: SimpleMessageBoxBuilder::new("BridgeMonitor", 1),
            ready,
        }
    }
}

impl RuntimeRequestSink for BridgeMonitorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.messages.get_signal_sender()
    }
}

impl Builder<BridgeMonitorActor> for BridgeMonitorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<BridgeMonitorActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> BridgeMonitorActor {
        BridgeMonitorActor {
            source: self.source,
            messages: self.messages.build(),
            ready: self.ready,
        }
    }
}

pub struct BridgeMonitorActor {
    source: BridgeSource,
    messages: SimpleMessageBox<MqttMessage, MqttMessage>,
    ready: watch::Sender<bool>,
}

#[async_trait]
impl Actor for BridgeMonitorActor {
    fn name(&self) -> &str {
        "BridgeMonitor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        if !self.wait_until_bridge_is_ready().await {
            return Ok(());
        }
        self.ready.send_replace(true);

        // The readiness never goes back on itself, so there is nothing left to watch. This actor
        // stays for as long as the messages it subscribed to keep coming: the broker republishes
        // the bridge health on every reconnection, and dropping the receiver would fail the MQTT
        // actor publishing to it.
        while self.messages.recv().await.is_some() {}
        Ok(())
    }
}

impl BridgeMonitorActor {
    /// Waits until the bridge can relay cloud-bound messages, returning `false` if it never can
    async fn wait_until_bridge_is_ready(&mut self) -> bool {
        match &mut self.source {
            // The built-in bridge raises this once it has a session with the local broker holding
            // the subscriptions it relays. From then on the broker queues the cloud-bound messages
            // it cannot deliver rather than discarding them.
            BridgeSource::BuiltIn(subscribed) => loop {
                if *subscribed.borrow_and_update() {
                    return true;
                }
                tokio::select! {
                    // The bridge stopped without ever subscribing
                    outcome = subscribed.changed() => if outcome.is_err() {
                        return false;
                    },

                    // Nothing is published here for the built-in bridge, so this only resolves
                    // when the runtime asks the monitor to shut down
                    message = self.messages.recv() => if message.is_none() {
                        return false;
                    },
                }
            },

            // A health message is the first sign that the broker has loaded the bridge
            // configuration, and so that it is queuing the cloud-bound messages published to it
            // even while the cloud is unreachable
            BridgeSource::InBroker {
                mqtt_schema,
                health_topic,
            } => loop {
                let Some(message) = self.messages.recv().await else {
                    // Shut down before the bridge was seen
                    return false;
                };
                if is_c8y_bridge_established(&message, mqtt_schema, health_topic) {
                    return true;
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_mapper_config;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::WithTimeout;
    use tedge_actors::Sender;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT: Duration = Duration::from_millis(500);

    /// The monitor exists for the connection it holds back: one built without holding anything
    /// back would leave the mapper publishing to topics the bridge does not yet relay
    #[test]
    fn a_monitored_connection_is_held_back_as_soon_as_the_monitor_is_built() {
        let (_subscribed, monitored) = watch::channel(false);
        let mut connection = GatedConnection::default();

        let _monitor = BridgeMonitorBuilder::built_in(monitored, &mut connection);

        assert!(
            !*connection.readiness().borrow(),
            "the bridge has yet to be seen, so the connection stays closed"
        );
        assert_eq!(connection.released_after(), MAX_BRIDGE_WAIT);
    }

    #[tokio::test]
    async fn built_in_bridge_is_not_ready_until_it_is_subscribed() {
        let (subscribed, monitored) = watch::channel(false);
        let mut connection = GatedConnection::default();
        let _shutdown =
            spawn_bridge_monitor(BridgeMonitorBuilder::built_in(monitored, &mut connection));
        let mut readiness = connection.readiness();

        assert!(
            readiness
                .changed()
                .with_timeout(TEST_TIMEOUT)
                .await
                .is_err(),
            "the bridge is not subscribed, so nothing may be published to the cloud yet"
        );

        subscribed.send_replace(true);
        assert!(is_ready(&mut readiness).await);
    }

    #[tokio::test]
    async fn built_in_bridge_is_never_ready_once_it_has_stopped() {
        let (subscribed, monitored) = watch::channel(false);
        let mut connection = GatedConnection::default();
        let _shutdown =
            spawn_bridge_monitor(BridgeMonitorBuilder::built_in(monitored, &mut connection));
        let mut readiness = connection.readiness();

        drop(subscribed);

        assert!(
            !is_ready(&mut readiness).await,
            "a bridge that stopped before subscribing will never relay anything"
        );
    }

    /// Whoever waits on the readiness has to be released when the runtime shuts the process down,
    /// however far the bridge got
    #[tokio::test]
    async fn a_bridge_never_seen_is_never_ready_once_the_monitor_is_shut_down() {
        let (_subscribed, monitored) = watch::channel(false);
        let mut connection = GatedConnection::default();
        let mut shutdown =
            spawn_bridge_monitor(BridgeMonitorBuilder::built_in(monitored, &mut connection));
        let mut readiness = connection.readiness();

        shutdown.send(RuntimeRequest::Shutdown).await.unwrap();

        assert!(!is_ready(&mut readiness).await);
    }

    #[tokio::test]
    async fn bridge_in_broker_is_ready_once_it_publishes_its_health() {
        let ttd = TempTedgeDir::new();
        let config = test_mapper_config(&ttd);
        let health_topic = config.bridge_health_topic.clone();

        let mut service_monitor: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("ServiceMonitor", 1);
        let mut connection = GatedConnection::default();
        let monitor =
            BridgeMonitorBuilder::in_broker(&mut service_monitor, &config, &mut connection);
        let _shutdown = spawn_bridge_monitor(monitor);
        let mut readiness = connection.readiness();
        let mut service_monitor = service_monitor.build();

        assert!(
            readiness.changed().with_timeout(TEST_TIMEOUT).await.is_err(),
            "the broker has given no sign of the bridge, so nothing may be published to the cloud yet"
        );

        // The bridge in the broker is a mosquitto bridge, whose health payload is 1 or 0
        service_monitor
            .send(MqttMessage::new(&health_topic, "1"))
            .await
            .unwrap();

        assert!(is_ready(&mut readiness).await);
    }

    #[tokio::test]
    async fn bridge_in_broker_ignores_the_health_of_other_services() {
        let ttd = TempTedgeDir::new();
        let config = test_mapper_config(&ttd);

        let mut service_monitor: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("ServiceMonitor", 1);
        let mut connection = GatedConnection::default();
        let monitor =
            BridgeMonitorBuilder::in_broker(&mut service_monitor, &config, &mut connection);
        let _shutdown = spawn_bridge_monitor(monitor);
        let mut readiness = connection.readiness();
        let mut service_monitor = service_monitor.build();

        service_monitor
            .send(MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/tedge-agent/status/health"),
                json!({"status": "up"}).to_string(),
            ))
            .await
            .unwrap();

        assert!(
            readiness
                .changed()
                .with_timeout(TEST_TIMEOUT)
                .await
                .is_err(),
            "only the bridge's own health says whether the bridge can relay messages"
        );
    }

    /// Spawns a monitor, returning the handle to shut it down
    fn spawn_bridge_monitor(builder: BridgeMonitorBuilder) -> DynSender<RuntimeRequest> {
        let signal_sender = builder.get_signal_sender();
        tokio::spawn(builder.build().run());
        signal_sender
    }

    /// Waits for the bridge to be ready, returning `false` if the monitor gave up on it first
    async fn is_ready(readiness: &mut watch::Receiver<bool>) -> bool {
        readiness
            .wait_for(|ready| *ready)
            .with_timeout(TEST_TIMEOUT)
            .await
            .expect("the monitor makes up its mind about the bridge")
            .is_ok()
    }

    /// Stands in for the MQTT connection a monitor is built for, recording how it is held back
    #[derive(Default)]
    struct GatedConnection {
        held_back_on: Option<(watch::Receiver<bool>, Duration)>,
    }

    impl GateConnection for GatedConnection {
        fn connect_when(&mut self, ready: watch::Receiver<bool>, timeout: Duration) {
            self.held_back_on = Some((ready, timeout));
        }
    }

    impl GatedConnection {
        /// The readiness this connection waits on, panicking if it was never held back at all
        fn readiness(&self) -> watch::Receiver<bool> {
            self.held_back().0.clone()
        }

        /// How long this connection waits before giving up on the bridge and connecting anyway
        fn released_after(&self) -> Duration {
            self.held_back().1
        }

        fn held_back(&self) -> &(watch::Receiver<bool>, Duration) {
            self.held_back_on
                .as_ref()
                .expect("the monitor holds back the connection it is built for")
        }
    }
}
