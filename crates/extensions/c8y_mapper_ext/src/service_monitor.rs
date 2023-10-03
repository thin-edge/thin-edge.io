use c8y_api::smartrest;
use serde::Deserialize;
use serde::Serialize;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityType;
use tedge_api::EntityStore;
use tedge_mqtt_ext::Message;

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct HealthStatus {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "unknown".to_string()
}

// TODO: instead of passing entity store, pass information about parent as part of the entity
// also reduce number of arguments
pub fn convert_health_status_message(
    entity_store: &EntityStore,
    entity: &EntityMetadata,
    message: &Message,
) -> Vec<Message> {
    if entity.r#type != EntityType::Service {
        return vec![];
    }

    let mut mqtt_messages: Vec<Message> = Vec::new();

    // If not Bridge health status
    if entity.topic_id.as_str().contains("bridge") {
        return mqtt_messages;
    }

    let HealthStatus {
        status: mut health_status,
    } = serde_json::from_slice(message.payload()).unwrap_or_default();

    if health_status.is_empty() {
        health_status = "unknown".into();
    }

    // TODO: make a "smartrest payload" type that contains appropriately escaped and sanitised data
    let mut health_status = smartrest::message::sanitize_for_smartrest(
        health_status.into_bytes(),
        smartrest::message::MAX_PAYLOAD_LIMIT_IN_BYTES,
    );

    if health_status.contains(',') {
        health_status = format!(r#""{health_status}""#);
    }

    let service_name = entity
        .other
        .get("name")
        .and_then(|n| n.as_str())
        .or(entity.topic_id.default_service_name())
        .unwrap_or(entity.external_id.as_ref());

    let service_type = entity
        .other
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("service");

    let ancestors_external_ids = entity_store
        .ancestors_external_ids(&entity.topic_id)
        .unwrap();

    let status_message = c8y_api::smartrest::inventory::service_creation_message(
        entity.external_id.as_ref(),
        service_name,
        service_type,
        &health_status,
        &ancestors_external_ids,
    );

    mqtt_messages.push(status_message);

    mqtt_messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::Topic;
    use test_case::test_case;
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","status":"up"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,up"#;
        "service-monitoring-thin-edge-device"
    )]
    #[test_case(
        "test_device",
        "te/device/child/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","status":"up"}"#,
        "c8y/s/us/test_device:device:child",
        r#"102,test_device:device:child:service:tedge-mapper-c8y,service,tedge-mapper-c8y,up"#;
        "service-monitoring-thin-edge-child-device"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"123456"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,unknown"#;
        "service-monitoring-thin-edge-no-status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":""}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,unknown"#;
        "service-monitoring-empty-status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        "{}",
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,unknown"#;
        "service-monitoring-empty-health-message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"up,down"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,"up,down""#;
        "service-monitoring-type-with-comma-health-message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"up\"down"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,up""down"#;
        "service-monitoring-double-quotes-health-message"
    )]
    fn translate_health_status_to_c8y_service_monitoring_message(
        device_name: &str,
        health_topic: &str,
        health_payload: &str,
        c8y_monitor_topic: &str,
        c8y_monitor_payload: &str,
    ) {
        let topic = Topic::new_unchecked(health_topic);

        let mqtt_schema = MqttSchema::new();
        let (entity, _) = mqtt_schema.entity_channel_of(&topic).unwrap();

        let health_message = Message::new(&topic, health_payload.as_bytes().to_owned());
        let expected_message = Message::new(
            &Topic::new_unchecked(c8y_monitor_topic),
            c8y_monitor_payload.as_bytes(),
        );

        let main_device_registration =
            EntityRegistrationMessage::main_device(device_name.to_string());
        let mut entity_store = EntityStore::with_main_device(
            main_device_registration,
            crate::converter::CumulocityConverter::map_to_c8y_external_id,
        )
        .unwrap();

        let entity_registration = EntityRegistrationMessage {
            topic_id: entity.clone(),
            external_id: None,
            r#type: EntityType::Service,
            parent: None,
            other: serde_json::json!({}),
        };

        entity_store
            .auto_register_entity(&entity_registration.topic_id)
            .unwrap();
        entity_store.update(entity_registration).unwrap();

        let entity = entity_store.get(&entity).unwrap();

        let msg = convert_health_status_message(&entity_store, entity, &health_message);
        assert_eq!(msg[0], expected_message);
    }
}
