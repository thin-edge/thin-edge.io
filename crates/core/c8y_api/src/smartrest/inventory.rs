//! This module provides some helper functions to create SmartREST messages
//! that can be used to create various managed objects in Cumulocity inventory.

// TODO: Have different SmartREST messages be different types, so we can see
// where these messages are used, not only created.
//
// TODO: both `C8yTopic::smartrest_response_topic(&EntityMetadata)` and
// `publish_topic_from_ancestors(&[String])` produce C8y MQTT topics on which
// smartrest messages are sent. There should be one comprehensive API for
// generating them.

use crate::smartrest::csv::fields_to_csv_string;
use crate::smartrest::topic::publish_topic_from_ancestors;
use mqtt_channel::MqttMessage;
use tedge_config::TopicPrefix;

use super::message::sanitize_for_smartrest;

/// Create a SmartREST message for creating a child device under the given ancestors.
/// The provided ancestors list must contain all the parents of the given device
/// starting from its immediate parent device.
pub fn child_device_creation_message(
    child_id: &str,
    device_name: Option<&str>,
    device_type: Option<&str>,
    ancestors: &[String],
    prefix: &TopicPrefix,
) -> Result<MqttMessage, InvalidValueError> {
    if child_id.is_empty() {
        return Err(InvalidValueError {
            field_name: "child_id".to_string(),
            value: child_id.to_string(),
        });
    }
    if let Some("") = device_name {
        return Err(InvalidValueError {
            field_name: "device_name".to_string(),
            value: "".to_string(),
        });
    }
    if let Some("") = device_type {
        return Err(InvalidValueError {
            field_name: "device_type".to_string(),
            value: "".to_string(),
        });
    }

    Ok(MqttMessage::new(
        &publish_topic_from_ancestors(ancestors, prefix),
        // XXX: if any arguments contain commas, output will be wrong
        format!(
            "101,{},{},{}",
            child_id,
            device_name.unwrap_or(child_id),
            device_type.unwrap_or("thin-edge.io-child")
        ),
    ))
}

/// Create a SmartREST message for creating a service on device.
/// The provided ancestors list must contain all the parents of the given service
/// starting from its immediate parent device.
pub fn service_creation_message(
    service_id: &str,
    service_name: &str,
    service_type: &str,
    service_status: &str,
    ancestors: &[String],
    prefix: &TopicPrefix,
) -> Result<MqttMessage, InvalidValueError> {
    // TODO: most of this noise can be eliminated by implementing `Serialize`/`Deserialize` for smartrest format
    if service_id.is_empty() {
        return Err(InvalidValueError {
            field_name: "service_id".to_string(),
            value: service_id.to_string(),
        });
    }
    if service_name.is_empty() {
        return Err(InvalidValueError {
            field_name: "service_name".to_string(),
            value: service_name.to_string(),
        });
    }
    if service_type.is_empty() {
        return Err(InvalidValueError {
            field_name: "service_type".to_string(),
            value: service_type.to_string(),
        });
    }
    if service_status.is_empty() {
        return Err(InvalidValueError {
            field_name: "service_status".to_string(),
            value: service_status.to_string(),
        });
    }

    Ok(MqttMessage::new(
        &publish_topic_from_ancestors(ancestors, prefix),
        fields_to_csv_string(&[
            "102",
            service_id,
            service_type,
            service_name,
            service_status,
        ]),
    ))
}

/// Create a SmartREST message for updating service status.
///
/// `service_status` can be any string, but `"up"`, `"down"`, and `"unknown"`
/// have known meanings and are displayed in the UI in different ways.
///
/// `external_ids` differs from what is returned by `ancestors_external_ids` in
/// that it also contains the external ID of the current entity (the one we want
/// to set the status of).
///
/// https://cumulocity.com/guides/reference/smartrest-two/#104
pub fn service_status_update_message(
    external_ids: &[impl AsRef<str>],
    service_status: &str,
    prefix: &TopicPrefix,
) -> MqttMessage {
    let topic = publish_topic_from_ancestors(external_ids, prefix);

    let service_status =
        sanitize_for_smartrest(service_status, super::message::MAX_PAYLOAD_LIMIT_IN_BYTES);

    MqttMessage::new(&topic, fields_to_csv_string(&["104", &service_status]))
}

#[derive(thiserror::Error, Debug)]
#[error("Field `{field_name}` contains invalid value: {value:?}")]
pub struct InvalidValueError {
    field_name: String,
    value: String,
}
