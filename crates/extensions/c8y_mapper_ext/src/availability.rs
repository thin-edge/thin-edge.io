use crate::actor::SyncStart;
use crate::actor::TimeoutKind;
use crate::service_monitor::get_health_status_from_message;
use crate::service_monitor::HealthStatus;
use std::collections::HashMap;
use std::time::Duration;
use tedge_actors::LoggingSender;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::ServiceTopicId;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::debug;
use tracing::info;

// TODO: Find better name
type TopicWithoutPrefix = String;

/// The timer payload for TimeoutKind::Heartbeat.
///
/// `device` should hold the EntityTopicId of the device for availability monitoring
/// `health` should hold the Topic of the device's lead service.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HeartbeatPayload {
    pub device: EntityTopicId,
    pub health: TopicWithoutPrefix,
}
pub fn create_c8y_heartbeat_message(
    map: &HashMap<TopicWithoutPrefix, HealthStatus>,
    c8y_topic: &Topic,
    heartbeat: &HeartbeatPayload,
) -> Option<MqttMessage> {
    let health_topic_name = heartbeat.health.clone();

    match map.get(&health_topic_name) {
        Some(health_status) if health_status.status == "up" => {
            Some(MqttMessage::new(c8y_topic, "{}"))
        }
        _ => {
            debug!("Heartbeat message is not sent because the reported status is not 'up'");
            None
        }
    }
}

pub async fn set_heartbeat_timer(
    period: i16,
    map: &mut HashMap<TopicWithoutPrefix, HealthStatus>,
    metadata: &EntityMetadata,
    timer_sender: LoggingSender<SyncStart>,
) {
    if period < 0 {
        return;
    }

    let interval: u64 = period.try_into().unwrap();
    if let Some(topic) = get_lead_service_topic(metadata) {
        insert_new_health_status_entry(map, &topic);

        start_heartbeat_timer(
            timer_sender.clone(),
            interval,
            metadata.topic_id.clone(),
            topic,
        )
        .await
        .unwrap(); // FIXME: Address RuntimeError
    } else {
        info!(
            "Couldn't start a timer for device availability heartbeat for the device '{}'",
            metadata.topic_id
        );
    }
}

pub async fn start_heartbeat_timer(
    mut timer_sender: LoggingSender<SyncStart>,
    interval: u64,
    device_entity: EntityTopicId,
    health_topic: TopicWithoutPrefix,
) -> Result<(), RuntimeError> {
    let heartbeat_payload = HeartbeatPayload {
        device: device_entity,
        health: health_topic,
    };

    timer_sender
        .send(SyncStart::new(
            Duration::from_secs(interval * 60), // interval is in minutes
            TimeoutKind::Heartbeat(heartbeat_payload),
        ))
        .await?;

    Ok(())
}

pub fn get_lead_service_topic(entity: &EntityMetadata) -> Option<TopicWithoutPrefix> {
    match entity.other.get("@health") {
        Some(maybe_topic_name) => match maybe_topic_name.as_str() {
            Some(topic_name) if Topic::new(topic_name).is_ok() => Some(topic_name.to_string()),
            _ => None,
        },
        None => entity
            .topic_id
            .to_default_service_topic_id("tedge-agent")
            .map(|service_topic| get_status_health_topic_id(service_topic.entity())),
    }
}

pub fn get_status_health_topic_id(topic_id: &EntityTopicId) -> TopicWithoutPrefix {
    format!(
        "{id}/{channel}",
        id = topic_id.as_str(),
        channel = Channel::Health
    )
}

pub fn default_main_lead_service_topic(entity: &EntityTopicId) -> TopicWithoutPrefix {
    let service_topic_id = entity
        .to_default_service_topic_id("tedge-agent")
        .unwrap_or(ServiceTopicId::new(entity.clone()));

    get_status_health_topic_id(service_topic_id.entity())
}

pub fn insert_new_health_status_entry(
    map: &mut HashMap<TopicWithoutPrefix, HealthStatus>,
    topic: &TopicWithoutPrefix,
) {
    map.insert(topic.clone(), HealthStatus::default());
}

/// When the given message's topic name is already registered to the given map as a key,
/// this function updates the entry according to the new health status.
pub fn record_health_status(
    mqtt_schema: &MqttSchema,
    map: &mut HashMap<TopicWithoutPrefix, HealthStatus>,
    message: &MqttMessage,
) {
    if MqttSchema::from_topic(&message.topic).root == mqtt_schema.root {
        let health_topic = message.topic.name.strip_prefix("te/").unwrap(); // FIXME
        if map.get(health_topic).is_some() {
            let status = get_health_status_from_message(message);
            map.insert(health_topic.to_string(), status);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tedge_api::entity_store::EntityMetadata;
    use test_case::test_case;

    #[test_case("te/device/main/service/tedge-agent/status/health", "up")]
    #[test_case("te/custom/topic", "up")]
    #[test_case("te/device/main/service/tedge-agent/status/health", "down")]
    #[test_case("te/custom/topic", "down")]
    #[test_case("te/device/main/service/tedge-agent/status/health", "any")]
    #[test_case("te/custom/topic", "any")]
    fn add_a_new_health_status_record(topic_name: &str, status: &str) {
        let mqtt_schema = MqttSchema::default();
        let mut map: HashMap<String, HealthStatus> = HashMap::new();
        let topic_without_prefix = topic_name.strip_prefix("te/").unwrap().to_string();
        insert_new_health_status_entry(&mut map, &topic_without_prefix);

        let message = MqttMessage::new(
            &Topic::new_unchecked(topic_name),
            json!({"status": status}).to_string(),
        );
        record_health_status(&mqtt_schema, &mut map, &message);
        dbg!(&message);
        dbg!(&map);
        let recorded = map.get(topic_name.strip_prefix("te/").unwrap()).unwrap();
        assert_eq!(recorded.status, status);
    }

    #[test_case("te/device/main/service/tedge-agent/status/health", "up")]
    #[test_case("te/custom/topic", "up")]
    fn not_add_a_new_health_status_record(topic_name: &str, status: &str) {
        let mqtt_schema = MqttSchema::default();
        let mut map: HashMap<String, HealthStatus> = HashMap::new();
        let topic = Topic::new_unchecked(topic_name);

        let message = MqttMessage::new(&topic, json!({"status": status}).to_string());
        record_health_status(&mqtt_schema, &mut map, &message);
        assert!(map.get(topic_name).is_none());
    }

    #[test_case("device/child1/service/tedge-agent/status/health")]
    #[test_case("any/valid/mqtt/topic")]
    fn get_custom_lead_service_topic(topic_name: &str) {
        let mut metadata = EntityMetadata::child_device("child1".into()).unwrap();
        metadata.other.insert("@health".into(), json!(topic_name));
        let topic = get_lead_service_topic(&metadata).unwrap();
        assert_eq!(topic, topic_name);
    }

    #[test]
    fn get_default_lead_service_topic_without_lead_service_declaration() {
        let metadata = EntityMetadata::child_device("child1".into()).unwrap();
        let topic = get_lead_service_topic(&metadata).unwrap();
        assert_eq!(topic, "device/child1/service/tedge-agent/status/health");
    }

    #[test]
    fn get_none_with_invalid_lead_service_topic() {
        let mut metadata = EntityMetadata::child_device("child1".into()).unwrap();
        metadata
            .other
            .insert("@health".into(), json!("invalid/mqtt/+/topic/#"));
        let topic = get_lead_service_topic(&metadata);
        assert_eq!(topic, None);
    }
}
