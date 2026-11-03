use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::commands::ControlCmd;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::warn;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    pub async fn handle_control_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.service_control {
            warn!("Received a control command, however, service_control feature is disabled");
            return Ok(OperationOutcome::Ignored);
        }

        let command = match ControlCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a control command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        match command.status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                let smartrest_set_operation = self.get_smartrest_successful_status_payload(
                    CumulocitySupportedOperations::C8yServiceCommand,
                    cmd_id,
                );

                Ok(OperationOutcome::Finished {
                    messages: vec![MqttMessage::new(
                        &target.smartrest_publish_topic,
                        smartrest_set_operation,
                    )],
                })
            }
            CommandStatus::Failed { reason } => {
                let action = &command.payload.action;
                Err(anyhow::anyhow!("The '{action}' action failed: {reason}").into())
            }
            _ => {
                // The other states are ignored
                Ok(OperationOutcome::Ignored)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::converter::tests::c8y_converter_config;
    use crate::converter::tests::create_c8y_converter;
    use crate::converter::tests::create_c8y_converter_from_config;
    use crate::converter::tests::register_source_entities;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::spawn_c8y_mapper_actor_with_config;
    use crate::tests::test_mapper_config;
    use crate::tests::TestHandle;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
    use tedge_mqtt_ext::test_helpers::test_mqtt_box::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    const SERVICE_XID: &str = "test-device:device:main:service:collectd";
    const SMARTREST_TOPIC: &str = "c8y/s/us/test-device:device:main:service:collectd";
    const INVENTORY_TOPIC: &str =
        "c8y/inventory/managedObjects/update/test-device:device:main:service:collectd";

    const CONTROL_CAPABILITY_TOPIC: &str = "te/device/main/service/collectd/cmd/control";
    const OPERATION_ID: &str = "1234";
    const CONTROL_CMD_TOPIC: &str = "te/device/main/service/collectd/cmd/control/c8y-mapper-1234";
    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn a_control_capability_registers_the_service_command_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                    r#"{"actions":["start"]}"#,
                ))
                .await,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["start"]}).into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn declared_actions_are_published_with_their_case_and_order() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                    r#"{"actions":["stop","requestLog","start"]}"#,
                ))
                .await,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["stop", "requestLog", "start"]}).into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn an_action_name_breaking_the_rule_is_not_declared() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                    r#"{"actions":["restart","do something"]}"#,
                ))
                .await,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["restart"]}).into(),
                ),
            ],
        );
    }

    #[test_case("{}"; "without actions")]
    #[test_case(r#"{"actions":"restart"}"#; "with actions not in an array")]
    #[test_case("restart"; "not json")]
    #[tokio::test]
    async fn an_invalid_control_capability_is_ignored(payload: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert!(converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                payload
            ))
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn clearing_the_control_capability_publishes_an_empty_c8y_supported_service_commands() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                    "",
                ))
                .await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": []}).into(),
            )],
        );
    }

    #[test_case(""; "cleared")]
    #[test_case(r#"{"actions":[]}"#; "declared empty")]
    #[test_case(r#"{"actions":["stop"]}"#; "replaced")]
    #[tokio::test]
    async fn an_action_no_longer_declared_is_refused(capability: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                capability,
            ))
            .await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {
                        "command": "restart",
                        "serviceName": "collectd",
                        "serviceType": "service",
                    },
                    "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert_operation_failed(
            &messages,
            SMARTREST_TOPIC,
            "has not declared the 'restart' action",
        );
    }

    #[tokio::test]
    async fn a_service_registered_again_does_not_inherit_the_previous_actions() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        converter
            .process_entity_metadata_message(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd"),
                "",
            ))
            .await
            .unwrap();
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {
                        "command": "restart",
                        "serviceName": "collectd",
                        "serviceType": "service",
                    },
                    "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert_operation_failed(
            &messages,
            SMARTREST_TOPIC,
            "has not declared the 'restart' action",
        );
    }

    #[tokio::test]
    async fn a_restart_capability_of_a_service_is_not_a_service_command() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                    "{}",
                ))
                .await,
            [(SMARTREST_TOPIC, "114,c8y_Restart".into())],
        );
    }

    #[test_case("restart"; "a standard action")]
    #[test_case("requestLog"; "an action in mixed case")]
    #[tokio::test]
    async fn a_c8y_service_command_operation_is_mapped_to_a_control_command_of_the_service(
        action: &str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                json!({"actions": [action]}).to_string(),
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": OPERATION_ID,
                        "status": "PENDING",
                        "c8y_ServiceCommand": {
                            "command": action,
                            "serviceName": "collectd",
                        },
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                CONTROL_CMD_TOPIC,
                json!({
                    "status": "init",
                    "action": action,
                    "serviceName": "collectd",
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[test_case(
        json!({"@type": "service", "type": "container"}),
        json!({"command": "restart", "serviceName": "collectd", "serviceType": "systemd"}),
        "systemd";
        "from the operation"
    )]
    #[test_case(
        json!({"@type": "service", "type": "container"}),
        json!({"command": "restart", "serviceName": "collectd"}),
        "container";
        "registered"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "restart", "serviceName": "collectd"}),
        "service";
        "default"
    )]
    #[tokio::test]
    async fn the_service_type_is_taken_from_the_operation_the_registration_or_the_default(
        registration: serde_json::Value,
        request: serde_json::Value,
        expected_type: &str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        converter
            .process_entity_metadata_message(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd"),
                registration.to_string(),
            ))
            .await
            .unwrap();
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": OPERATION_ID,
                        "status": "PENDING",
                        "c8y_ServiceCommand": request,
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                CONTROL_CMD_TOPIC,
                json!({
                    "status": "init",
                    "action": "restart",
                    "serviceName": "collectd",
                    "serviceType": expected_type,
                })
                .into(),
            )],
        );
    }

    #[test_case(
        json!({"command": "restart", "serviceName": "collectd"}),
        "collectd";
        "from the operation"
    )]
    #[test_case(json!({"command": "restart"}), "collectd-daemon"; "registered")]
    #[tokio::test]
    async fn the_service_name_is_taken_from_the_operation_or_the_registration(
        request: serde_json::Value,
        expected_name: &str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        converter
            .process_entity_metadata_message(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd"),
                json!({"@type": "service", "name": "collectd-daemon"}).to_string(),
            ))
            .await
            .unwrap();
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": OPERATION_ID,
                        "status": "PENDING",
                        "c8y_ServiceCommand": request,
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                CONTROL_CMD_TOPIC,
                json!({
                    "status": "init",
                    "action": "restart",
                    "serviceName": expected_name,
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[test_case(
        "unknown-service",
        "c8y/s/us/unknown-service",
        "unknown-service";
        "an unknown target"
    )]
    #[test_case("test-device", "c8y/s/us", "has not declared"; "the main device")]
    #[tokio::test]
    async fn a_failure_is_reported_on_the_smartrest_topic_of_the_target(
        target_xid: &str,
        smartrest_topic: &str,
        reason: &str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {
                        "command": "restart",
                        "serviceName": "collectd",
                        "serviceType": "service",
                    },
                    "externalSource": {"externalId": target_xid, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert_operation_failed(&messages, smartrest_topic, reason);
    }

    #[tokio::test]
    async fn no_status_is_sent_for_a_target_xid_that_makes_an_invalid_smartrest_topic() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {"command": "restart", "serviceName": "collectd"},
                    "externalSource": {"externalId": "unknown+service", "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert!(messages.is_empty(), "{messages:?}");
    }

    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "stop", "serviceName": "collectd", "serviceType": "service"}),
        "has not declared the 'stop'";
        "an undeclared action"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "service"}),
        "has not declared the 'RESTART'";
        "a declared action in another case"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "", "serviceName": "collectd", "serviceType": "service"}),
        "Invalid action name";
        "an empty action"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "do something", "serviceName": "collectd", "serviceType": "service"}),
        "Invalid action name";
        "an action with a space"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "restart.now", "serviceName": "collectd", "serviceType": "service"}),
        "Invalid action name";
        "an action with a dot"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "restart", "serviceType": "service"}),
        "neither the operation nor the registration of the service gives a service name";
        "no service name"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "restart", "serviceName": "--now", "serviceType": "service"}),
        "Invalid service name";
        "a service name looking like an option"
    )]
    #[test_case(
        json!({"@type": "service", "name": "--now"}),
        json!({"command": "restart", "serviceType": "service"}),
        "Invalid service name";
        "a registered name looking like an option"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "restart", "serviceName": "collectd", "serviceType": "../../bin/sh"}),
        "Invalid service type";
        "a service type naming no plugin file"
    )]
    #[test_case(
        json!({"@type": "service", "type": "../../bin/sh"}),
        json!({"command": "restart", "serviceName": "collectd"}),
        "Invalid service type";
        "a registered type naming no plugin file"
    )]
    #[tokio::test]
    async fn an_invalid_operation_is_failed_without_a_command(
        registration: serde_json::Value,
        request: serde_json::Value,
        reason: &str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        converter
            .process_entity_metadata_message(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd"),
                registration.to_string(),
            ))
            .await
            .unwrap();
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": request,
                    "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, reason);
    }

    #[tokio::test]
    async fn a_failure_is_reported_with_the_operation_id_when_configured() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.smartrest_use_operation_id = true;
        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {"command": "restart", "serviceName": "collectd"},
                    "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        let [executing, failed] = &messages[..] else {
            panic!("expecting only 504 and 505: {messages:?}");
        };
        assert_eq!(executing.topic.name, SMARTREST_TOPIC);
        assert_eq!(executing.payload_str().unwrap(), "504,1234");
        assert_eq!(failed.topic.name, SMARTREST_TOPIC);
        let failed = failed.payload_str().unwrap();
        assert!(failed.starts_with("505,1234,"), "{failed}");
        assert!(
            failed.contains("has not declared the 'restart' action"),
            "{failed}"
        );
    }

    #[test_case("Collectd-Daemon"; "a name other than the topic segment")]
    #[test_case("dbus-:1.2-org.freedesktop.problems@0"; "a systemd unit name holding a colon")]
    #[test_case("Nginx Web Server"; "a display name holding spaces")]
    #[test_case("collectd;reboot"; "holding a shell separator")]
    #[tokio::test]
    async fn the_service_name_of_the_operation_is_used_in_the_command(service_name: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": OPERATION_ID,
                        "status": "PENDING",
                        "c8y_ServiceCommand": {
                            "command": "restart",
                            "serviceName": service_name,
                        },
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                CONTROL_CMD_TOPIC,
                json!({
                    "status": "init",
                    "action": "restart",
                    "serviceName": service_name,
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn no_action_is_declared_when_service_control_is_disabled() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.capabilities.service_control = false;
        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert!(converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#
            ))
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn an_operation_is_ignored_when_service_control_is_disabled() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.capabilities.service_control = false;
        let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
                r#"{"actions":["restart"]}"#,
            ))
            .await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {
                        "command": "restart",
                        "serviceName": "collectd",
                        "serviceType": "service",
                    },
                    "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                })
                .to_string(),
            ))
            .await;

        assert!(messages.is_empty());
    }

    #[test_case(
        json!({"status": "successful", "action": "restart", "serviceName": "collectd", "serviceType": "service"}),
        "503,c8y_ServiceCommand";
        "successful"
    )]
    #[test_case(
        json!({"status": "failed", "action": "restart", "serviceName": "collectd", "serviceType": "service", "reason": "failed with reason"}),
        "502,c8y_ServiceCommand,The 'restart' action failed: failed with reason";
        "failed"
    )]
    #[tokio::test]
    async fn the_status_of_a_service_command_is_reported_on_the_topic_of_the_service(
        final_state: serde_json::Value,
        smartrest: &str,
    ) {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd"),
            r#"{ "@type": "service" }"#,
        ))
        .await
        .unwrap();
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(CONTROL_CAPABILITY_TOPIC),
            r#"{"actions":["restart"]}"#,
        ))
        .await
        .unwrap();

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("c8y/devicecontrol/notifications"),
            json!({
                "id": OPERATION_ID,
                "status": "PENDING",
                "c8y_ServiceCommand": {
                    "command": "restart",
                    "serviceName": "collectd",
                    "serviceType": "service"
                },
                "externalSource": {
                    "externalId": SERVICE_XID,
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .unwrap();

        let topic = Topic::new_unchecked(CONTROL_CMD_TOPIC);
        assert_received_contains_str(&mut mqtt, [(topic.name.as_str(), r#""status":"init""#)])
            .await;

        mqtt.send(MqttMessage::new(
            &topic,
            json!({"status": "executing", "action": "restart", "serviceName": "collectd", "serviceType": "service"})
                .to_string(),
        ))
        .await
        .unwrap();
        assert_received_contains_str(&mut mqtt, [(SMARTREST_TOPIC, "501,c8y_ServiceCommand")])
            .await;

        mqtt.send(MqttMessage::new(&topic, final_state.to_string()))
            .await
            .unwrap();
        assert_received_contains_str(&mut mqtt, [(SMARTREST_TOPIC, smartrest)]).await;
    }

    #[tokio::test]
    async fn a_control_command_status_is_ignored_when_service_control_is_disabled() {
        let ttd = TempTedgeDir::new();
        let mut config = test_mapper_config(&ttd);
        config.capabilities.service_control = false;
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd"),
            r#"{ "@type": "service" }"#,
        ))
        .await
        .unwrap();
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(CONTROL_CMD_TOPIC),
            json!({"status": "executing", "action": "restart", "serviceName": "collectd", "serviceType": "service"})
                .to_string(),
        ))
        .await
        .unwrap();

        while let Some(message) = mqtt.recv().await {
            assert_ne!(message.topic.name, SMARTREST_TOPIC, "{message:?}");
        }
    }

    fn assert_operation_failed(messages: &[MqttMessage], topic: &str, reason: &str) {
        let [executing, failed] = messages else {
            panic!("expecting only 501 and 502: {messages:?}");
        };
        assert_eq!(executing.topic.name, topic);
        assert_eq!(executing.payload_str().unwrap(), "501,c8y_ServiceCommand");

        assert_eq!(failed.topic.name, topic);
        let failed = failed.payload_str().unwrap();
        assert!(failed.starts_with("502,c8y_ServiceCommand,"), "{failed}");
        assert!(failed.contains(reason), "{failed}");
    }
}
