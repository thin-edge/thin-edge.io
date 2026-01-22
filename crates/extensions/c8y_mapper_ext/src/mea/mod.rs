use crate::entity_cache::CloudEntityMetadata;
use crate::json::Units;
use tedge_api::entity::EntityMetadata;
use tedge_flows::FlowContextHandle;
use tedge_flows::JsonValue;

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
