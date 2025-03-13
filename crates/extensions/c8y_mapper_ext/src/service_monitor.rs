use crate::converter::create_get_pending_operations_message;
use crate::entity_cache::CloudEntityMetadata;
use c8y_api::smartrest;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::HealthStatus;
use tedge_api::Status;
use tedge_config::models::TopicPrefix;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::error;

pub fn is_c8y_bridge_established(
    message: &MqttMessage,
    mqtt_schema: &MqttSchema,
    bridge_service_topic: &Topic,
) -> bool {
    if let Ok(health_status) = HealthStatus::try_from_health_status_message(message, mqtt_schema) {
        &message.topic == bridge_service_topic && health_status.is_valid()
    } else {
        false
    }
}

pub(crate) fn convert_health_status_message(
    mqtt_schema: &MqttSchema,
    entity: &CloudEntityMetadata,
    parent_xid: Option<&EntityExternalId>,
    main_device_xid: &EntityExternalId,
    message: &MqttMessage,
    prefix: &TopicPrefix,
) -> Vec<MqttMessage> {
    // TODO: introduce type to remove entity type guards
    if entity.metadata.r#type != EntityType::Service {
        return vec![];
    }

    let HealthStatus { status } =
        HealthStatus::try_from_health_status_message(message, mqtt_schema).unwrap();

    let external_id = entity.external_id.as_ref();
    let display_name = entity
        .metadata
        .display_name()
        .or_else(|| entity.metadata.topic_id.default_service_name())
        .unwrap_or(external_id);

    let display_type = entity.metadata.display_type().unwrap_or("service");

    let Ok(status_message) = smartrest::inventory::service_creation_message(
        external_id,
        display_name,
        display_type,
        &status.to_string(),
        parent_xid.map(|v| v.as_ref()),
        main_device_xid.as_ref(),
        prefix,
    ) else {
        error!("Can't create 102 for service status update");
        return vec![];
    };

    let mut value = vec![status_message];

    if display_name == format!("mosquitto-{prefix}-bridge") && status == Status::Up {
        // Receiving this message indicates mosquitto has reconnected (following a
        // disconnection) to the cloud. We need to re-request operations in case any
        // were triggered while we were down
        value.push(create_get_pending_operations_message(prefix));
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converter::CumulocityConverter;
    use serde_json::Map;
    use tedge_api::entity::EntityMetadata;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_mqtt_ext::Topic;
    use test_case::test_case;

    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"pid":1234,"status":"up"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,up"#;
        "service-monitoring-thin-edge-device"
    )]
    // If there are any problems with fields other than `status`, we want to ignore them and still send status update
    #[test_case(
        "test_device",
        "te/device/main/service/tedge-mapper-c8y/status/health",
        r#"{"unrecognised_field": [42], "time": "invalid timestamp", "pid": "invalid pid", "status": "up"}"#,
        "c8y/s/us",
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,up"#;
        "service-monitoring-thin-edge-device-optional-fields-invalid"
    )]
    #[test_case(
        "test_device",
        "te/device/child/service/tedge-mapper-c8y/status/health",
        r#"{"pid":1234,"status":"up"}"#,
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
        r#"102,test_device:device:main:service:tedge-mapper-c8y,service,tedge-mapper-c8y,"up""down""#;
        "service-monitoring-double-quotes-health-message"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "1",
        "c8y/s/us",
        r#"102,test_device:device:main:service:mosquitto-xyz-bridge,service,mosquitto-xyz-bridge,up"#;
        "service-monitoring-mosquitto-bridge-up-status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "0",
        "c8y/s/us",
        r#"102,test_device:device:main:service:mosquitto-xyz-bridge,service,mosquitto-xyz-bridge,down"#;
        "service-monitoring-mosquitto-bridge-down-status"
    )]
    #[test_case(
        "test_device",
        "te/device/main/service/mosquitto-xyz-bridge/status/health",
        "invalid payload",
        "c8y/s/us",
        r#"102,test_device:device:main:service:mosquitto-xyz-bridge,service,mosquitto-xyz-bridge,unknown"#;
        "service-monitoring-mosquitto-bridge-unknown-status"
    )]
    fn translate_health_status_to_c8y_service_monitoring_message(
        main_device_id: &str,
        health_topic: &str,
        health_payload: &str,
        c8y_monitor_topic: &str,
        c8y_monitor_payload: &str,
    ) {
        let topic = Topic::new_unchecked(health_topic);

        let mqtt_schema = MqttSchema::new();
        let (entity_topic_id, _) = mqtt_schema.entity_channel_of(&topic).unwrap();

        let health_message = MqttMessage::new(&topic, health_payload.as_bytes().to_owned());
        let expected_message = MqttMessage::new(
            &Topic::new_unchecked(c8y_monitor_topic),
            c8y_monitor_payload.as_bytes(),
        );

        let external_id =
            CumulocityConverter::map_to_c8y_external_id(&entity_topic_id, &"test_device".into());

        let parent = entity_topic_id.default_service_parent_identifier();
        let parent_xid = parent
            .clone()
            .map(|tid| CumulocityConverter::map_to_c8y_external_id(&tid, &"test_device".into()));

        let entity = CloudEntityMetadata::new(
            external_id.clone(),
            EntityMetadata {
                topic_id: entity_topic_id,
                external_id: Some(external_id),
                r#type: EntityType::Service,
                parent,
                twin_data: Map::new(),
            },
        );

        let msg = convert_health_status_message(
            &mqtt_schema,
            &entity,
            parent_xid.as_ref(),
            &main_device_id.into(),
            &health_message,
            &"c8y".try_into().unwrap(),
        );
        assert_eq!(msg[0], expected_message);
    }

    const C8Y_BRIDGE_HEALTH_TOPIC: &str =
        "te/device/main/service/mosquitto-c8y-bridge/status/health";

    #[test_case(C8Y_BRIDGE_HEALTH_TOPIC, "1", true)]
    #[test_case(C8Y_BRIDGE_HEALTH_TOPIC, "0", true)]
    #[test_case(C8Y_BRIDGE_HEALTH_TOPIC, "bad payload", false)]
    #[test_case("tedge/not/health/topic", "1", false)]
    #[test_case("tedge/not/health/topic", "0", false)]
    fn test_bridge_is_established(topic: &str, payload: &str, expected: bool) {
        let mqtt_schema = MqttSchema::default();
        let topic = Topic::new(topic).unwrap();
        let message = MqttMessage::new(&topic, payload);

        let actual = is_c8y_bridge_established(
            &message,
            &mqtt_schema,
            &C8Y_BRIDGE_HEALTH_TOPIC.try_into().unwrap(),
        );
        assert_eq!(actual, expected);
    }
}
