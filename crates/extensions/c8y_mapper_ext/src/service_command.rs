use crate::converter::CumulocityConverter;
use crate::entity_cache::CloudEntityMetadata;
use crate::error::ConversionError;
use c8y_api::json_c8y_deserializer::C8yServiceCommand;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_id;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_name;
use c8y_api::smartrest::topic::C8yTopic;
use serde_json::json;
use std::collections::HashMap;
use tedge_api::commands::ControlCmd;
use tedge_api::commands::ControlCmdMetadata;
use tedge_api::commands::ControlCmdPayload;
use tedge_api::commands::Jsonify;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::service_command::ActionName;
use tedge_api::service_command::ServiceName;
use tedge_api::service_command::ServiceType;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::warn;

const C8Y_SERVICE_COMMAND: &str = "c8y_ServiceCommand";
const C8Y_SUPPORTED_SERVICE_COMMANDS: &str = "c8y_SupportedServiceCommands";

#[derive(Debug, Default)]
pub struct ServiceCommands {
    actions_by_service: HashMap<EntityTopicId, Vec<ActionName>>,
}

impl ServiceCommands {
    pub(crate) fn remove_service(&mut self, service: &EntityTopicId) {
        self.actions_by_service.remove(service);
    }

    fn contains_action(&self, service: &EntityTopicId, action: &ActionName) -> bool {
        self.actions_by_service
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }
}

impl CumulocityConverter {
    pub(crate) async fn convert_service_control_metadata(
        &mut self,
        service: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.service_commands {
            warn!(
                "Received service command metadata, however, service_commands feature is disabled"
            );
            return Ok(vec![]);
        }

        let mut messages = vec![];

        let actions = if message.payload_bytes().is_empty() {
            vec![]
        } else {
            let capability = match ControlCmdMetadata::from_slice(message.payload_bytes()) {
                Ok(capability) => capability,
                Err(err) => {
                    warn!(
                        topic = %message.topic.name,
                        "Ignoring the control capability of a service: {err}"
                    );
                    return Ok(vec![]);
                }
            };

            // The operation directory of a service is not watched,
            // so the operation is registered here
            match self.register_operation(service, C8Y_SERVICE_COMMAND).await {
                Ok(mut registration) => messages.append(&mut registration),
                Err(err) => error!(
                    "Failed to register the `{C8Y_SERVICE_COMMAND}` operation of {service}: {err}"
                ),
            }

            capability
                .actions
                .into_iter()
                .filter_map(|action| match action.parse::<ActionName>() {
                    Ok(action) => Some(action),
                    Err(err) => {
                        warn!(
                            topic = %message.topic.name,
                            "Not declaring the action of a service to Cumulocity: {err}"
                        );
                        None
                    }
                })
                .collect()
        };

        messages.push(self.inventory_update_message(
            service,
            json!({ C8Y_SUPPORTED_SERVICE_COMMANDS: actions }),
        )?);
        self.service_commands
            .actions_by_service
            .insert(service.clone(), actions);

        Ok(messages)
    }

    pub(crate) fn convert_service_command_request(
        &self,
        device_xid: String,
        cmd_id: String,
        op_id: &str,
        request: C8yServiceCommand,
    ) -> Vec<MqttMessage> {
        let target_xid: EntityExternalId = device_xid.into();
        let target = match self.entity_cache.try_get_by_external_id(&target_xid) {
            Ok(target) => target,
            Err(err) => {
                return self.fail_service_command(
                    &target_xid,
                    EntityType::Service,
                    op_id,
                    &err.to_string(),
                )
            }
        };

        let (action, service_name, service_type) =
            match self.validate_service_command(target, &request) {
                Ok(validated) => validated,
                Err(reason) => {
                    return self.fail_service_command(&target_xid, target.r#type(), op_id, &reason)
                }
            };

        let command = ControlCmd {
            target: target.topic_id().clone(),
            cmd_id,
            payload: ControlCmdPayload {
                status: CommandStatus::Init,
                action: action.to_string(),
                service_name: service_name.to_string(),
                service_type: service_type.to_string(),
            },
        };

        vec![command.command_message(&self.mqtt_schema)]
    }

    /// Checks a `c8y_ServiceCommand` request and resolves the service name and type to run it with
    ///
    /// Returns the reason to fail the operation with when the request cannot be run.
    fn validate_service_command(
        &self,
        target: &CloudEntityMetadata,
        request: &C8yServiceCommand,
    ) -> Result<(ActionName, ServiceName, ServiceType), String> {
        let xid = target.external_id.as_ref();
        let command = &request.command;

        let action = command
            .parse::<ActionName>()
            .map_err(|err| format!("{xid} cannot run the command '{command}': {err}"))?;
        if !self
            .service_commands
            .contains_action(target.topic_id(), &action)
        {
            return Err(format!("{xid} has not declared the '{action}' action"));
        }

        let service_name = Some(request.service_name.as_str())
            .filter(|name| !name.is_empty())
            .or_else(|| target.registered_name())
            .ok_or_else(|| {
                format!("{xid} cannot run the '{action}' action: neither the operation nor the registration of the service gives a service name")
            })?;
        // `tedge service` validates the name too,
        // but only a check here reports the reason on the operation
        let service_name = service_name
            .parse::<ServiceName>()
            .map_err(|err| format!("{xid} cannot run the '{action}' action: {err}"))?;

        let service_type = Some(request.service_type.as_str())
            .filter(|ty| !ty.is_empty())
            .or_else(|| target.registered_type())
            .unwrap_or(DEFAULT_SERVICE_TYPE)
            .parse::<ServiceType>()
            .map_err(|err| format!("{xid} cannot run the '{action}' action: {err}"))?;

        Ok((action, service_name, service_type))
    }

    fn fail_service_command(
        &self,
        target_xid: &EntityExternalId,
        entity_type: EntityType,
        op_id: &str,
        reason: &str,
    ) -> Vec<MqttMessage> {
        error!("Rejecting a {C8Y_SERVICE_COMMAND} operation: {reason}");

        let prefix = &self.config.bridge_config.c8y_prefix;
        let Some(topic) = C8yTopic::smartrest_response_topic(target_xid, &entity_type, prefix)
        else {
            // Unlike the external ids of registered entities,
            // this one comes from the cloud unchecked
            error!(
                "Not reporting the failure to Cumulocity: '{}' names no SmartREST topic",
                target_xid.as_ref()
            );
            return vec![];
        };

        let (executing, failed) = if self.config.smartrest_use_operation_id {
            (
                set_operation_executing_with_id(op_id),
                fail_operation_with_id(op_id, reason),
            )
        } else {
            (
                set_operation_executing_with_name(C8Y_SERVICE_COMMAND),
                fail_operation_with_name(C8Y_SERVICE_COMMAND, reason),
            )
        };

        vec![
            MqttMessage::new(&topic, executing),
            MqttMessage::new(&topic, failed),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::converter::tests::c8y_converter_config;
    use crate::converter::tests::create_c8y_converter;
    use crate::converter::tests::create_c8y_converter_from_config;
    use crate::converter::tests::register_source_entities;
    use serde_json::json;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
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
    async fn no_action_is_declared_when_service_commands_are_disabled() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.capabilities.service_commands = false;
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
    async fn an_operation_is_ignored_when_service_commands_are_disabled() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = c8y_converter_config(&tmp_dir);
        config.capabilities.service_commands = false;
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
