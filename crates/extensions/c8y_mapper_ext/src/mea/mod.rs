use crate::entity_cache::CloudEntityMetadata;
use crate::json::Units;
use crate::mea::entities::C8yEntityBirth;
use tedge_api::entity::EntityMetadata;
use tedge_api::store::RingBuffer;
use tedge_flows::FlowContextHandle;
use tedge_flows::JsonValue;
use tedge_flows::Message;

pub mod alarms;
pub mod entities;
pub mod events;
pub mod measurements;

fn get_entity_metadata(context: &FlowContextHandle, entity: &str) -> Option<CloudEntityMetadata> {
    let json = context.get_value(entity);
    if json == JsonValue::Null {
        return None;
    }
    let metadata: EntityMetadata = json.into_value().ok()?;
    let external_id = metadata.external_id.as_ref()?.to_owned();
    Some(CloudEntityMetadata {
        external_id,
        metadata,
    })
}

fn get_measurement_units(
    context: &FlowContextHandle,
    root: &str,
    entity: &str,
    measurement_type: &str,
) -> Option<Units> {
    let key = format!("{root}/{entity}/m/{measurement_type}/meta");
    let json = context.get_value(&key);
    if json == JsonValue::Null {
        return None;
    }
    let metadata = json.into_value().ok()?;
    Some(Units::from_metadata(metadata))
}

fn take_cached_telemetry_data(
    cache: &mut RingBuffer<Message>,
    birth_payload: &str,
) -> Vec<Message> {
    let Ok(birth_message) = C8yEntityBirth::from_json(birth_payload) else {
        return vec![];
    };

    let mut messages = vec![];
    let pending_messages = cache.take();
    for message in pending_messages.into_iter() {
        if message.topic.starts_with(&birth_message.topic) {
            messages.push(message);
        } else {
            cache.push(message);
        }
    }
    messages
}
