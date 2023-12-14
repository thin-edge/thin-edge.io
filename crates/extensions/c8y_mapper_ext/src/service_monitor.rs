use c8y_api::smartrest;
use serde::Deserialize;
use serde::Serialize;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityType;
use tedge_mqtt_ext::Message;

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct HealthStatus {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "unknown".to_string()
}

pub fn convert_health_status_message(
    entity: &EntityMetadata,
    ancestors_external_ids: &[String],
    message: &Message,
) -> Vec<Message> {
    // TODO: introduce type to remove entity type guards
    if entity.r#type != EntityType::Service {
        return vec![];
    }

    let mut mqtt_messages: Vec<Message> = Vec::new();

    // If not Bridge health status
    // FIXME: can also match "device/bridge//" or "/device/main/service/my_custom_bridge"
    // should match ONLY the single mapper bridge
    if entity.topic_id.as_str().contains("bridge") {
        return mqtt_messages;
    }

    let HealthStatus {
        status: mut health_status,
    } = serde_json::from_slice(message.payload()).unwrap_or_default();

    if health_status.is_empty() {
        health_status = "unknown".into();
    }

    // FIXME: `ancestors_external_ids` gives external ids starting from the parent, but for health
    // we need XID of current device as well
    let mut external_ids = vec![entity.external_id.as_ref().to_string()];
    external_ids.extend_from_slice(ancestors_external_ids);
    let status_message =
        smartrest::inventory::service_status_update_message(&external_ids, &health_status);

    mqtt_messages.push(status_message);

    mqtt_messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::EntityStore;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::Topic;
    use test_case::test_case;
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","status":"up"}"#,
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,up"#;
        "service monitoring thin-edge device"
    )]
    #[test_case(
        "test_device",
        "te/device/child/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"1234","status":"up"}"#,
        "c8y/s/us/test_device:device:child/test_device:device:child:service:tedge-mapper-c8y",
        r#"104,up"#;
        "service monitoring thin-edge child device"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":"123456"}"#,
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,unknown"#;
        "service monitoring thin-edge no status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":""}"#,
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,unknown"#;
        "service monitoring empty status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        "{}",
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,unknown"#;
        "service monitoring empty health message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"up,down"}"#,
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,"up,down""#;
        "service monitoring type with comma health message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"status":"up\"down"}"#,
        "c8y/s/us/test_device:device:main:service:tedge-mapper-c8y",
        r#"104,"up""down""#;
        "service monitoring double quotes health message"
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
        let (entity_topic_id, _) = mqtt_schema.entity_channel_of(&topic).unwrap();

        let health_message = Message::new(&topic, health_payload.as_bytes().to_owned());
        let expected_message = Message::new(
            &Topic::new_unchecked(c8y_monitor_topic),
            c8y_monitor_payload.as_bytes(),
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let main_device_registration =
            EntityRegistrationMessage::main_device(device_name.to_string());
        let mut entity_store = EntityStore::with_main_device_and_default_service_type(
            MqttSchema::default(),
            main_device_registration,
            "service".into(),
            crate::converter::CumulocityConverter::map_to_c8y_external_id,
            crate::converter::CumulocityConverter::validate_external_id,
            5,
            &temp_dir,
        )
        .unwrap();

        let entity_registration = EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            external_id: None,
            r#type: EntityType::Service,
            parent: None,
            other: serde_json::Map::new(),
        };

        entity_store
            .auto_register_entity(&entity_registration.topic_id)
            .unwrap();
        entity_store.update(entity_registration).unwrap();

        let entity = entity_store.get(&entity_topic_id).unwrap();
        let ancestors_external_ids = entity_store
            .ancestors_external_ids(&entity_topic_id)
            .unwrap();

        let msg = convert_health_status_message(entity, &ancestors_external_ids, &health_message);
        assert_eq!(msg[0], expected_message);
    }
}
