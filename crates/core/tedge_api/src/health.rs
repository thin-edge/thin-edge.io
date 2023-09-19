use std::process;
use std::sync::Arc;

use crate::mqtt_topics::ServiceTopicId;
use mqtt_channel::Message;
use mqtt_channel::PubChannel;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use serde_json::json;
use time::OffsetDateTime;

/// Encodes a valid health topic.
///
/// Health topics are topics on which messages about health status of services are published. To be
/// able to send health messages, a health topic needs to be constructed for a given entity.
// TODO: replace `Arc<str>` with `ServiceTopicId` after we're done with transition to new topics
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealthTopic(Arc<str>);

impl ServiceHealthTopic {
    pub fn new(service: ServiceTopicId) -> Self {
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
            "time": OffsetDateTime::now_utc().unix_timestamp(),
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
        let response_topic_health = Topic::new_unchecked(self.as_str());

        let health_status = json!({
            "status": "up",
            "pid": process::id(),
            "time": OffsetDateTime::now_utc().unix_timestamp(),
        })
        .to_string();

        Message::new(&response_topic_health, health_status)
            .with_qos(mqtt_channel::QoS::AtLeastOnce)
            .with_retain()
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
    let response_topic_health =
        Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

    let health_status = json!({
        "status": "up",
        "pid": process::id(),
        "time": OffsetDateTime::now_utc().unix_timestamp(),
    })
    .to_string();

    let health_message = Message::new(&response_topic_health, health_status).with_retain();
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
    let response_topic_health =
        Topic::new_unchecked(format!("tedge/health/{daemon_name}").as_str());

    let health_status = json!({
        "status": "up",
        "pid": process::id(),
        "time": OffsetDateTime::now_utc().unix_timestamp(),
    })
    .to_string();

    Message::new(&response_topic_health, health_status)
        .with_qos(mqtt_channel::QoS::AtLeastOnce)
        .with_retain()
}
