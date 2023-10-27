use crate::mqtt_topics::Channel;
use crate::mqtt_topics::MqttSchema;
use crate::mqtt_topics::ServiceTopicId;
use clock::Clock;
use clock::WallClock;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use serde_json::json;
use std::process;
use std::sync::Arc;

/// Encodes a valid health topic.
///
/// Health topics are topics on which messages about health status of services are published. To be
/// able to send health messages, a health topic needs to be constructed for a given entity.
// Because all the services use the same `HealthMonitorActor`, `ServiceHealthTopic` needs to support
// both old and new topics until all the services are fully moved to the new topic scheme.
//
// TODO: replace `Arc<str>` with `ServiceTopicId` after we're done with transition to new topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealthTopic(Arc<str>);

impl ServiceHealthTopic {
    /// Create a new `ServiceHealthTopic` from a topic in a new topic scheme.
    pub fn from_new_topic(service_topic_id: &ServiceTopicId, mqtt_schema: &MqttSchema) -> Self {
        let health_topic = mqtt_schema.topic_for(service_topic_id.entity(), &Channel::Health);
        Self(health_topic.name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn down_message(&self) -> Message {
        Message {
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

    pub fn up_message(&self) -> Message {
        let timestamp = WallClock
            .now()
            .format(&time::format_description::well_known::Rfc3339);
        match timestamp {
            Ok(timestamp) => {
                let health_status = json!({
                    "status": "up",
                    "pid": process::id(),
                    "time": timestamp
                })
                .to_string();

                let response_topic_health = Topic::new_unchecked(self.as_str());

                Message::new(&response_topic_health, health_status)
                    .with_qos(mqtt_channel::QoS::AtLeastOnce)
                    .with_retain()
            }
            Err(e) => {
                let error_topic = Topic::new_unchecked("tedge/errors");
                let error_msg = format!(
                    "Health message: Failed to convert timestamp to Rfc3339 format due to: {e}"
                );
                Message::new(&error_topic, error_msg).with_qos(mqtt_channel::QoS::AtLeastOnce)
            }
        }
    }
}

#[derive(Debug)]
pub struct HealthTopicError;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn is_rfc3339_timestamp() {
        let health_topic = ServiceHealthTopic(Arc::from(
            "te/device/main/service/test_daemon/status/health",
        ));
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
}
