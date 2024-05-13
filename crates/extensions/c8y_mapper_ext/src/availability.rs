use crate::actor::SyncStart;
use crate::actor::TimeoutKind;
use std::str::FromStr;
use std::time::Duration;
use tedge_actors::LoggingSender;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::mqtt_topics::EntityTopicId;
use tracing::warn;

/// The timer payload for TimeoutKind::Heartbeat.
///
/// `device` should hold the EntityTopicId of the device for availability monitoring
/// `service` should hold the EntityTopicId of the device's lead service.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HeartbeatPayload {
    pub device: EntityTopicId,
    pub service: EntityTopicId,
}

pub async fn start_heartbeat_timer(
    mut timer_sender: LoggingSender<SyncStart>,
    interval: u64,
    device_entity: EntityTopicId,
    service_entity: EntityTopicId,
) -> Result<(), RuntimeError> {
    let heartbeat_payload = HeartbeatPayload {
        device: device_entity,
        service: service_entity,
    };

    timer_sender
        .send(SyncStart::new(
            Duration::from_secs(interval * 60), // interval is in minutes
            TimeoutKind::Heartbeat(heartbeat_payload),
        ))
        .await?;

    Ok(())
}

// FIXME: How to support custom topic scheme?
// Test 'custom_topic_scheme_registration_mapping' is failing
// because the new entity topic ID is "custom///"
// What is the default service of "custom///"? "custom//service/tedge-agent"? Error?
pub fn get_child_lead_service_entity_topic_id(
    entity_metadata: &EntityMetadata,
) -> Option<EntityTopicId> {
    if let Some(maybe_service_entity_topic_id) = entity_metadata.other.get("leadService") {
        if let Some(service_entity) = maybe_service_entity_topic_id.as_str() {
            if let Ok(entity_id) = EntityTopicId::from_str(service_entity) {
                return Some(entity_id);
            }
        }
    }

    if let Some(device_name) = entity_metadata.topic_id.default_device_name() {
        warn!("Given 'leadService' is malformed. Using the default tedge-agent service topic scheme instead");
        return Some(default_child_lead_service(device_name));
    }

    None
}

fn default_child_lead_service(child: &str) -> EntityTopicId {
    EntityTopicId::default_child_service(child, "tedge-agent")
        .expect("Call this function only if the child name is surely valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tedge_api::entity_store::EntityMetadata;

    #[test]
    fn get_custom_lead_service_entity_topic_id() {
        let mut metadata = EntityMetadata::child_device("child1".into()).unwrap();
        metadata
            .other
            .insert("leadService".into(), json!("device/child1/service/foo"));
        let entity = get_child_lead_service_entity_topic_id(&metadata).unwrap();
        assert_eq!(
            entity,
            EntityTopicId::default_child_service("child1", "foo").unwrap()
        );
    }

    #[test]
    fn get_default_lead_service_entity_topic_id_with_invalid_lead_service_value() {
        let mut metadata = EntityMetadata::child_device("child1".into()).unwrap();
        metadata
            .other
            .insert("leadService".into(), json!("device/child1/too/many/args"));
        let entity = get_child_lead_service_entity_topic_id(&metadata).unwrap();
        assert_eq!(
            entity,
            EntityTopicId::default_child_service("child1", "tedge-agent").unwrap()
        );
    }

    #[test]
    fn get_default_lead_service_entity_topic_id_without_lead_service_declaration() {
        let metadata = EntityMetadata::child_device("child1".into()).unwrap();
        let entity = get_child_lead_service_entity_topic_id(&metadata).unwrap();
        assert_eq!(
            entity,
            EntityTopicId::default_child_service("child1", "tedge-agent").unwrap()
        );
    }

    #[test]
    fn no_lead_service_entity_topic_id_without_device_name() {
        let mut metadata = EntityMetadata::child_device("".into()).unwrap();
        metadata.topic_id = EntityTopicId::from_str("custom///").unwrap();
        let entity = get_child_lead_service_entity_topic_id(&metadata);
        assert_eq!(entity, None);
    }

    #[test]
    fn nos_lead_service_entity_topic_id_without_device_name() {
        let mut metadata = EntityMetadata::child_device("".into()).unwrap();
        metadata.topic_id = EntityTopicId::from_str("custom/child1//").unwrap();
        let entity = get_child_lead_service_entity_topic_id(&metadata);
        assert_eq!(entity, None);
    }
}
