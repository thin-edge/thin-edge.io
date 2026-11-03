//! Detects the domain names of the tenant again whenever the bridge connects to Cumulocity
//!
//! The bridge only receives the certificate served by Cumulocity when it connects,
//! so a reconnection is when a replaced certificate listing new domain names,
//! e.g. during a domain rename, is first seen by the device.
//! Both a built-in bridge and a bridge running in the broker publish their health on the local
//! broker, each time they connect.

use crate::config::C8yMapperConfig;
use async_trait::async_trait;
use c8y_api::domain_names::DomainNameRefresh;
use std::convert::Infallible;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSource;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::HealthStatus;
use tedge_api::Status;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub struct DomainNameRefreshBuilder {
    mqtt_schema: MqttSchema,
    health_topic: Topic,
    refresh: DomainNameRefresh,
    messages: SimpleMessageBoxBuilder<MqttMessage, NoMessage>,
}

impl DomainNameRefreshBuilder {
    /// Requests `refresh` each time the bridge health, received from `service_monitor`, is up
    pub fn new(
        service_monitor: &mut impl MessageSource<MqttMessage, TopicFilter>,
        config: &C8yMapperConfig,
        refresh: DomainNameRefresh,
    ) -> Self {
        let messages = SimpleMessageBoxBuilder::new("DomainNameRefresh", 1);
        service_monitor.connect_sink(config.bridge_health_topic.clone().into(), &messages);
        Self {
            mqtt_schema: config.mqtt_schema.clone(),
            health_topic: config.bridge_health_topic.clone(),
            refresh,
            messages,
        }
    }
}

impl RuntimeRequestSink for DomainNameRefreshBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.messages.get_signal_sender()
    }
}

impl Builder<DomainNameRefreshActor> for DomainNameRefreshBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<DomainNameRefreshActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> DomainNameRefreshActor {
        DomainNameRefreshActor {
            mqtt_schema: self.mqtt_schema,
            health_topic: self.health_topic,
            refresh: self.refresh,
            messages: self.messages.build(),
        }
    }
}

pub struct DomainNameRefreshActor {
    mqtt_schema: MqttSchema,
    health_topic: Topic,
    refresh: DomainNameRefresh,
    messages: SimpleMessageBox<MqttMessage, NoMessage>,
}

#[async_trait]
impl Actor for DomainNameRefreshActor {
    fn name(&self) -> &str {
        "DomainNameRefresh"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(message) = self.messages.recv().await {
            if is_bridge_up(&message, &self.mqtt_schema, &self.health_topic) {
                self.refresh.request();
            }
        }
        Ok(())
    }
}

fn is_bridge_up(message: &MqttMessage, mqtt_schema: &MqttSchema, health_topic: &Topic) -> bool {
    &message.topic == health_topic
        && HealthStatus::try_from_health_status_message(message, mqtt_schema)
            .is_ok_and(|health| health.status == Status::Up)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_mapper_config;
    use assert_matches::assert_matches;
    use rstest::rstest;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::WithTimeout;
    use tedge_actors::Sender;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::sync::watch;

    const TEST_TIMEOUT: Duration = Duration::from_millis(500);
    const MOSQUITTO_BRIDGE: &str = "mosquitto-c8y-bridge";
    const BUILT_IN_BRIDGE: &str = "tedge-mapper-bridge-c8y";

    #[tokio::test]
    async fn refresh_is_requested_when_a_bridge_in_the_broker_is_up() {
        let (mut service_monitor, mut requests, health_topic) =
            spawn_domain_name_refresh(MOSQUITTO_BRIDGE);

        // A bridge in the broker is a mosquitto bridge, whose health payload is 1 or 0
        service_monitor
            .send(MqttMessage::new(&health_topic, "1"))
            .await
            .unwrap();

        assert_matches!(
            requests.changed().with_timeout(TEST_TIMEOUT).await,
            Ok(Ok(()))
        );
    }

    #[tokio::test]
    async fn refresh_is_requested_when_the_built_in_bridge_is_up() {
        let (mut service_monitor, mut requests, health_topic) =
            spawn_domain_name_refresh(BUILT_IN_BRIDGE);

        service_monitor
            .send(MqttMessage::new(
                &health_topic,
                json!({"status": "up"}).to_string(),
            ))
            .await
            .unwrap();

        assert_matches!(
            requests.changed().with_timeout(TEST_TIMEOUT).await,
            Ok(Ok(()))
        );
    }

    // A bridge in the broker is a mosquitto bridge, whose health payload is 1 or 0
    #[rstest]
    #[case::bridge_in_broker_up(MOSQUITTO_BRIDGE, MOSQUITTO_BRIDGE, "1", true)]
    #[case::bridge_in_broker_down(MOSQUITTO_BRIDGE, MOSQUITTO_BRIDGE, "0", false)]
    #[case::built_in_bridge_up(BUILT_IN_BRIDGE, BUILT_IN_BRIDGE, r#"{"status":"up"}"#, true)]
    #[case::built_in_bridge_down(BUILT_IN_BRIDGE, BUILT_IN_BRIDGE, r#"{"status":"down"}"#, false)]
    #[case::another_service_up(MOSQUITTO_BRIDGE, "tedge-agent", r#"{"status":"up"}"#, false)]
    fn bridge_is_up_only_when_its_own_health_is_up(
        #[case] bridge_service_name: &str,
        #[case] service_name: &str,
        #[case] payload: &str,
        #[case] expected: bool,
    ) {
        let message = MqttMessage::new(&health_topic(service_name), payload);

        assert_eq!(
            is_bridge_up(
                &message,
                &MqttSchema::default(),
                &health_topic(bridge_service_name)
            ),
            expected
        );
    }

    fn spawn_domain_name_refresh(
        bridge_service_name: &str,
    ) -> (
        SimpleMessageBox<MqttMessage, MqttMessage>,
        watch::Receiver<()>,
        Topic,
    ) {
        let ttd = TempTedgeDir::new();
        let mut config = test_mapper_config(&ttd);
        config.bridge_health_topic = health_topic(bridge_service_name);
        let (refresh, requests) = watch::channel(());
        let mut service_monitor: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("ServiceMonitor", 1);
        let actor =
            DomainNameRefreshBuilder::new(&mut service_monitor, &config, refresh.into()).build();
        tokio::spawn(actor.run());
        (
            service_monitor.build(),
            requests,
            config.bridge_health_topic.clone(),
        )
    }

    fn health_topic(service_name: &str) -> Topic {
        Topic::new_unchecked(&format!(
            "te/device/main/service/{service_name}/status/health"
        ))
    }
}
