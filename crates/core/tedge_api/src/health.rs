use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::ServiceTopicId;
use clock::Clock;
use clock::WallClock;
use log::error;
use mqtt_channel::MqttMessage;
use mqtt_channel::Topic;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::fmt::Display;
use std::process;
use std::sync::Arc;
use tedge_utils::timestamp::TimeFormat;

mod health_status;
pub use health_status::HealthStatus;

pub fn service_health_topic(
    mqtt_schema: &MqttSchema,
    device_topic_id: &EntityTopicId,
    service: &str,
) -> Topic {
    mqtt_schema.topic_for(
        &device_topic_id.default_service_for_device(service).unwrap(),
        &Channel::Health,
    )
}

/// Encodes a valid health topic.
///
/// Health topics are topics on which messages about health status of services are published. To be
/// able to send health messages, a health topic needs to be constructed for a given entity.
// Because all the services use the same `HealthMonitorActor`, `ServiceHealthTopic` needs to support
// both old and new topics until all the services are fully moved to the new topic scheme.
//
// TODO: replace `Arc<str>` with `ServiceTopicId` after we're done with transition to new topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealthTopic {
    topic: Arc<str>,
    time_format: TimeFormat,
}

impl ServiceHealthTopic {
    /// Create a new `ServiceHealthTopic` from a topic in a new topic scheme.
    pub fn from_new_topic(
        service_topic_id: &ServiceTopicId,
        mqtt_schema: &MqttSchema,
        time_format: TimeFormat,
    ) -> Self {
        let health_topic = mqtt_schema.topic_for(service_topic_id.entity(), &Channel::Health);
        Self {
            topic: health_topic.name.into(),
            time_format,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.topic
    }

    pub fn down_message(&self) -> MqttMessage {
        MqttMessage {
            topic: Topic::new_unchecked(self.as_str()),
            payload: json!({
                "status": "down",
                "pid": process::id()})
            .to_string()
            .into(),
            qos: mqtt_channel::QoS::AtLeastOnce,
            retain: true,
        }
    }

    pub fn up_message(&self) -> MqttMessage {
        let now = WallClock.now();
        let time_format = self.time_format;
        let timestamp = time_format.to_json(now).unwrap_or_else(|err| {
            error!(
                "Health message: Failed to convert timestamp to {time_format} format due to: {err}"
            );
            now.to_string().into()
        });

        let health_status = json!({
            "status": "up",
            "pid": process::id(),
            "time": timestamp
        })
        .to_string();

        let response_topic_health = Topic::new_unchecked(self.as_str());

        MqttMessage::new(&response_topic_health, health_status)
            .with_qos(mqtt_channel::QoS::AtLeastOnce)
            .with_retain()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Up,
    Down,
    #[serde(untagged)]
    Other(String),
}

impl Default for Status {
    fn default() -> Self {
        Status::Other("unknown".to_string())
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            Status::Up => "up",
            Status::Down => "down",
            Status::Other(val) if val.is_empty() => "unknown",
            Status::Other(val) => val,
        };
        write!(f, "{}", status)
    }
}

#[derive(Debug)]
pub struct HealthTopicError;

pub fn entity_is_mosquitto_bridge_service(entity_topic_id: &EntityTopicId) -> bool {
    entity_topic_id
        .default_service_name()
        .filter(|name| name.starts_with("mosquitto-") && name.ends_with("-bridge"))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::Value;
    use test_case::test_case;

    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"up"}"#,
        Status::Up;
        "service-health-status-up"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"down"}"#,
        Status::Down;
        "service-health-status-down"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"foo"}"#,
        Status::Other("foo".into());
        "service-health-status-other-value"
    )]
    #[test_case(
        "te/device/child/service/tedge-mapper-c8y/status/health",
        r#"{"pid":1234,"status":"up"}"#,
        Status::Up;
        "service-health-status-with-extra-fields"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"123456"}"#,
        Status::Other("unknown".into());
        "service-health-status-no-value"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":""}"#,
        Status::Other("".into());
        "service-health-status-empty-value"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-c8y/status/health",
        "{}",
        Status::default();
        "service-health-status-empty-message"
    )]
    #[test_case(
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "1",
        Status::Up;
        "mosquitto-bridge-service-health-status-up"
    )]
    #[test_case(
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "0",
        Status::Down;
        "mosquitto-bridge-service-health-status-down"
    )]
    #[test_case(
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "invalid payload",
        Status::default();
        "mosquitto-bridge-service-health-status-invalid-payload"
    )]
    #[test_case(
        "te/device/main/service/tedge-mapper-bridge-c8y/status/health",
        r#"{"status":"up"}"#,
        Status::Up;
        "builtin-bridge-service-health-status-up"
    )]
    fn parse_heath_status(health_topic: &str, health_payload: &str, expected_status: Status) {
        let mqtt_schema = MqttSchema::new();
        let topic = Topic::new_unchecked(health_topic);
        let health_message = MqttMessage::new(&topic, health_payload.as_bytes().to_owned());

        let health_status =
            HealthStatus::try_from_health_status_message(&health_message, &mqtt_schema);
        assert_eq!(health_status.unwrap().status, expected_status);
    }

    #[test]
    fn is_rfc3339_timestamp() {
        let health_topic = ServiceHealthTopic {
            topic: "te/device/main/service/test_daemon/status/health".into(),
            time_format: TimeFormat::Rfc3339,
        };
        let msg = health_topic.up_message();

        let health_msg_str = msg.payload_str().unwrap();
        let deserialized_value: Value =
            serde_json::from_str(health_msg_str).expect("Failed to parse JSON");
        let timestamp = deserialized_value.get("time").unwrap().as_str().unwrap();
        // The RFC3339 format pattern
        let pattern = r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{0,}Z$";

        // Use regex to check if the timestamp matches the pattern
        let regex = regex::Regex::new(pattern).unwrap();
        assert!(regex.is_match(timestamp));
    }

    #[test]
    fn is_unix_timestamp() {
        let health_topic = ServiceHealthTopic {
            topic: "te/device/main/service/test_daemon/status/health".into(),
            time_format: TimeFormat::Unix,
        };
        let msg = health_topic.up_message();

        let health_msg_str = msg.payload_str().unwrap();
        let deserialized_value: Value =
            serde_json::from_str(health_msg_str).expect("Failed to parse JSON");
        let timestamp = deserialized_value.get("time").unwrap();

        assert_matches!(timestamp, Value::Number(..))
    }
}
