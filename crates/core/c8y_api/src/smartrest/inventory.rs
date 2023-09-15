use crate::smartrest::topic::publish_topic_from_ancestors;
use mqtt_channel::Message;

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
