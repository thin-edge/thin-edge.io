use crate::entity_cache::CloudEntityMetadata;
use crate::json::Units;
use tedge_flows::FlowContextHandle;
use tedge_flows::JsonValue;

pub mod alarms;
pub mod entities;
pub mod events;
pub mod health;
pub mod measurements;
pub mod message_cache;

fn get_entity_metadata(context: &FlowContextHandle, entity: &str) -> Option<CloudEntityMetadata> {
    let json = context.get_value(entity);
    if json == JsonValue::Null {
        return None;
    }
    json.into_value().ok()
}

fn get_entity_parent_metadata(
    context: &FlowContextHandle,
    entity: &CloudEntityMetadata,
) -> Option<CloudEntityMetadata> {
    entity
        .parent()
        .and_then(|parent_tid| get_entity_metadata(context, parent_tid.as_str()))
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
