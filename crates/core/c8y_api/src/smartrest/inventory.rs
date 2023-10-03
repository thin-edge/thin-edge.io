//! This module provides some helper functions to create SmartREST messages
//! that can be used to create various managed objects in Cumulocity inventory.
use crate::smartrest::topic::publish_topic_from_ancestors;
use mqtt_channel::Message;

/// Create a SmartREST message for creating a child device under the given ancestors.
/// The provided ancestors list must contain all the parents of the given device
/// starting from its immediate parent device.
// XXX: if any arguments contain commas, output will be wrong
pub fn child_device_creation_message(
    child_id: &str,
    device_name: Option<&str>,
    device_type: Option<&str>,
    ancestors: &[String],
) -> Message {
    Message::new(
        &publish_topic_from_ancestors(ancestors),
        format!(
            "101,{},{},{}",
            child_id,
            device_name.unwrap_or(child_id),
            device_type.unwrap_or("thin-edge.io-child")
        ),
    )
}

/// Create a SmartREST message for creating a service on device.
/// The provided ancestors list must contain all the parents of the given service
/// starting from its immediate parent device.
// XXX: if any arguments contain commas, output will be wrong
pub fn service_creation_message(
    service_id: &str,
    service_name: &str,
    service_type: &str,
    service_status: &str,
    ancestors: &[String],
) -> Message {
    Message::new(
        &publish_topic_from_ancestors(ancestors),
        format!(
            "102,{},{},{},{}",
            service_id, service_type, service_name, service_status
        ),
    )
}
