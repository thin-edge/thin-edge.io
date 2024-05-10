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

pub fn get_child_lead_service_entity_topic_id(entity_metadata: &EntityMetadata) -> EntityTopicId {
    let device_name = entity_metadata.topic_id.default_device_name().unwrap(); // FIXME: Remove unwrap or always safe?

    // TODO: key is leadService or lead_service?
    match entity_metadata.other.get("leadService") {
        Some(service_entity_topic_id) => {
            let service_entity = service_entity_topic_id.to_string();
            match EntityTopicId::from_str(&service_entity) {
                Ok(entity_id) => entity_id,
                Err(_) => {
                    // FIXME: if the given 'leadSerivice' is invalid topic ID,
                    // should it use tedge-agent automatically or return an error?
                    warn!("Given leadService {service_entity} is invalid. Using tedge-agent as default");
                    default_child_lead_service(device_name)
                }
            }
        }
        None => default_child_lead_service(device_name),
    }
}

fn default_child_lead_service(child: &str) -> EntityTopicId {
    EntityTopicId::default_child_service(child, "tedge-agent")
        .expect("Call this function only if the child name is surely valid")
}
