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
use std::process;
use std::sync::Arc;
use tedge_utils::timestamp::TimeFormat;

pub const MOSQUITTO_BRIDGE_PREFIX: &str = "mosquitto-";
pub const MOSQUITTO_BRIDGE_SUFFIX: &str = "-bridge";
pub const MOSQUITTO_BRIDGE_UP_PAYLOAD: &str = "1";
pub const MOSQUITTO_BRIDGE_DOWN_PAYLOAD: &str = "0";

pub const UP_STATUS: &str = "up";
pub const DOWN_STATUS: &str = "down";
pub const UNKNOWN_STATUS: &str = "unknown";

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

#[derive(Debug)]
pub struct HealthTopicError;

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct HealthStatus {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "unknown".to_string()
}

impl HealthStatus {
    pub fn from_mosquitto_bridge_payload_str(payload: &str) -> Self {
        let status = match payload {
            MOSQUITTO_BRIDGE_UP_PAYLOAD => UP_STATUS,
            MOSQUITTO_BRIDGE_DOWN_PAYLOAD => DOWN_STATUS,
            _ => UNKNOWN_STATUS,
        };
        HealthStatus {
            status: status.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.status == UP_STATUS || self.status == DOWN_STATUS
    }
}

pub fn entity_is_mosquitto_bridge_service(entity_topic_id: &EntityTopicId) -> bool {
    entity_topic_id
        .default_service_name()
        .filter(|name| {
            name.starts_with(MOSQUITTO_BRIDGE_PREFIX) && name.ends_with(MOSQUITTO_BRIDGE_SUFFIX)
        })
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::Value;

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
