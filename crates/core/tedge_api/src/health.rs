use clock::Clock;
use clock::WallClock;
use std::process;
use std::sync::Arc;

use crate::mqtt_topics::ServiceTopicId;
use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use serde_json::json;

/// Encodes a valid health topic.
///
/// Health topics are topics on which messages about health status of services are published. To be
/// able to send health messages, a health topic needs to be constructed for a given entity.
// TODO: replace `Arc<str>` with `ServiceTopicId` after we're done with transition to new topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealthTopic(Arc<str>);

impl ServiceHealthTopic {
    pub fn new(service: ServiceTopicId) -> Self {
        // XXX: hardcoded MQTT root
        ServiceHealthTopic(Arc::from(format!("te/{}/status/health", service.as_str())))
    }

    pub fn from_old_topic(topic: String) -> Result<Self, HealthTopicError> {
        match topic.split('/').collect::<Vec<&str>>()[..] {
            ["tedge", "health", _service_name] => {}
            ["tedge", "health", _child_id, _service_name] => {}
            _ => return Err(HealthTopicError),
        }

        Ok(Self(Arc::from(topic)))
    }

    pub fn is_health_topic(topic: &str) -> bool {
        matches!(
            topic.split('/').collect::<Vec<&str>>()[..],
            ["te", _, _, _, _, "status", "health"]
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub async fn send_health_status(&self, responses: &mut impl PubChannel) {
        let response_topic_health = Topic::new_unchecked(self.as_str());

        let health_status = json!({
            "status": "up",
            "pid": process::id(),
        })
        .to_string();

        let health_message = Message::new(&response_topic_health, health_status).with_retain();
        let _ = responses.send(health_message).await;
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

// TODO: remove below functions once components moved to new health topics

pub fn health_check_topics(daemon_name: &str) -> TopicFilter {
    vec![
        "tedge/health-check".into(),
        format!("tedge/health-check/{daemon_name}"),
    ]
    .try_into()
    .expect("Invalid topic filter")
}

pub async fn send_health_status(responses: &mut impl PubChannel, daemon_name: &str) {
    let health_message = health_status_up_message(daemon_name);
    let _ = responses.send(health_message).await;
}

pub fn health_status_down_message(daemon_name: &str) -> Message {
    Message {
        topic: Topic::new_unchecked(&format!("tedge/health/{daemon_name}")),
        payload: json!({
            "status": "down",
            "pid": process::id()})
        .to_string()
        .into(),
        qos: mqtt_channel::QoS::AtLeastOnce,
        retain: true,
    }
}

pub fn health_status_up_message(daemon_name: &str) -> Message {
    let clock = Box::new(WallClock);
    let timestamp = clock
        .now()
        .format(&time::format_description::well_known::Rfc3339);
    match timestamp {
        Ok(time_stamp) => {
            let health_status = json!({
                "status": "up",
                "pid": process::id(),
                "time": time_stamp,
            })
            .to_string();
            let response_topic_health =
                Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

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

pub fn is_bridge_health(topic: &str) -> bool {
    if topic.starts_with("tedge/health") {
        let substrings: Vec<String> = topic.split('/').map(String::from).collect();
        if substrings.len() > 2 {
            let bridge_splits: Vec<&str> = substrings[2].split('-').collect();
            matches!(bridge_splits[..], ["mosquitto", _, "bridge"])
        } else {
            false
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::health_status_up_message;
    #[test]
    fn is_rfc3339_timestamp() {
        let msg = health_status_up_message("test_daemon");
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
