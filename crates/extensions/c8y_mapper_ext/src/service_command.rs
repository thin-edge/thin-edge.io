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
use std::collections::BTreeSet;
use std::collections::HashMap;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityType;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::service_command::ActionName;
use tedge_api::service_command::ServiceName;
use tedge_api::service_command::ServiceType;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::warn;

const C8Y_SERVICE_COMMAND: &str = "c8y_ServiceCommand";
const C8Y_SUPPORTED_SERVICE_COMMANDS: &str = "c8y_SupportedServiceCommands";

#[derive(Debug, Default)]
pub struct ServiceCommands {
    actions_by_service: HashMap<EntityTopicId, BTreeSet<String>>,
}

impl ServiceCommands {
    fn add_action(&mut self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .entry(service.clone())
            .or_default()
            .insert(action.to_string())
    }

    fn remove_action(&mut self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .get_mut(service)
            .is_some_and(|actions| actions.remove(action))
    }

    pub(crate) fn remove_service(&mut self, service: &EntityTopicId) {
        self.actions_by_service.remove(service);
    }

    fn contains_action(&self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }

    /// Cumulocity uppercases a command name by convention.
    fn c8y_names(&self, service: &EntityTopicId) -> Vec<String> {
        self.actions_by_service
            .get(service)
            .map(|actions| actions.iter().map(|action| action.to_uppercase()).collect())
            .unwrap_or_default()
    }
}

impl CumulocityConverter {
    pub(crate) async fn convert_service_command_metadata(
        &mut self,
        service: &EntityTopicId,
        operation: &OperationType,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let action = match operation.name().parse::<ActionName>() {
            Ok(action) => action,
            Err(err) => {
                warn!(
                    topic = %message.topic.name,
                    "Not declaring the action of a service to Cumulocity: {err}"
                );
                return Ok(vec![]);
            }
        };

        let cleared = message.payload_bytes().is_empty();
        let changed = if cleared {
            self.service_commands
                .remove_action(service, action.as_str())
        } else {
            self.service_commands.add_action(service, action.as_str())
        };
        if !changed {
            return Ok(vec![]);
        }

        let mut messages = vec![];

        if !cleared {
            // The operation directory of a service is not watched,
            // so the operation is registered here
            match self.register_operation(service, C8Y_SERVICE_COMMAND).await {
                Ok(mut registration) => messages.append(&mut registration),
                Err(err) => error!(
                    "Failed to register the `{C8Y_SERVICE_COMMAND}` operation of {service}: {err}"
                ),
            }
        }

        let supported_commands = self.service_commands.c8y_names(service);
        messages.push(self.inventory_update_message(
            service,
            json!({ C8Y_SUPPORTED_SERVICE_COMMANDS: supported_commands }),
        )?);

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

        let topic = self.mqtt_schema.topic_for(
            target.topic_id(),
            &Channel::Command {
                operation: action.as_str().into(),
                cmd_id,
            },
        );
        let payload = json!({
            "serviceName": service_name.as_str(),
            "serviceType": service_type.as_str(),
        });
        let command = GenericCommandState::new(topic, CommandStatus::Init.to_string(), payload)
            .into_message();

        vec![command]
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
            .to_lowercase()
            .parse::<ActionName>()
            .map_err(|err| format!("{xid} cannot run the command '{command}': {err}"))?;
        if !self
            .service_commands
            .contains_action(target.topic_id(), action.as_str())
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
    use crate::converter::tests::create_c8y_converter;
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

    const OPERATION_ID: &str = "1234";
    const RESTART_CMD_TOPIC: &str = "te/device/main/service/collectd/cmd/restart/c8y-mapper-1234";

    #[tokio::test]
    async fn the_first_declared_action_registers_the_service_command_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                    "{}",
                ))
                .await,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn declared_actions_are_published_in_uppercase_and_sorted() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        for action in ["start", "stop"] {
            converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked(&format!("te/device/main/service/collectd/cmd/{action}")),
                    "{}",
                ))
                .await;
        }

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                    "{}",
                ))
                .await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["RESTART", "START", "STOP"]}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn an_action_declared_again_publishes_nothing() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        let restart = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
            "{}",
        );
        converter.convert(&restart).await;

        assert!(converter.convert(&restart).await.is_empty());
    }

    #[tokio::test]
    async fn a_service_registered_again_does_not_inherit_the_previous_actions() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                "{}",
            ))
            .await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/stop"),
                "{}",
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

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                    "{}",
                ))
                .await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn an_action_name_with_an_uppercase_letter_is_not_declared() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        assert!(converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/doSomething"),
                "{}"
            ))
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn a_cleared_action_is_removed_from_c8y_supported_service_commands() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                "{}",
            ))
            .await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/pause"),
                "{}",
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/pause"),
                    "",
                ))
                .await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
            )],
        );

        assert!(converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/pause"),
                ""
            ))
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn clearing_the_last_action_publishes_an_empty_c8y_supported_service_commands() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;

        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                "{}",
            ))
            .await;

        assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("te/device/main/service/collectd/cmd/start"),
                    "",
                ))
                .await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": []}).into(),
            )],
        );
    }

    #[test_case("restart", "RESTART", RESTART_CMD_TOPIC; "restart")]
    #[test_case(
        "is-active",
        "IS-ACTIVE",
        "te/device/main/service/collectd/cmd/is-active/c8y-mapper-1234";
        "an action with a dash"
    )]
    #[tokio::test]
    async fn a_c8y_service_command_operation_is_mapped_to_a_command_of_the_service(
        action: &str,
        command: &str,
        cmd_topic: &'static str,
    ) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_source_entities("te/device/main/service/collectd", &mut converter).await;
        converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked(&format!("te/device/main/service/collectd/cmd/{action}")),
                "{}",
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
                            "command": command,
                            "serviceName": "collectd",
                        },
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                cmd_topic,
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[test_case(
        json!({"@type": "service", "type": "container"}),
        json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "systemd"}),
        "systemd";
        "from the operation"
    )]
    #[test_case(
        json!({"@type": "service", "type": "container"}),
        json!({"command": "RESTART", "serviceName": "collectd"}),
        "container";
        "registered"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "RESTART", "serviceName": "collectd"}),
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
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                "{}",
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
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": expected_type,
                })
                .into(),
            )],
        );
    }

    #[test_case(
        json!({"command": "RESTART", "serviceName": "collectd"}),
        "collectd";
        "from the operation"
    )]
    #[test_case(json!({"command": "RESTART"}), "collectd-daemon"; "registered")]
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
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                "{}",
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
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
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
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                "{}",
            ))
            .await;

        let messages = converter
            .convert(&MqttMessage::new(
                &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                json!({
                    "id": OPERATION_ID,
                    "status": "PENDING",
                    "c8y_ServiceCommand": {
                        "command": "RESTART",
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
        json!({"command": "STOP", "serviceName": "collectd", "serviceType": "service"}),
        "has not declared the 'stop'";
        "an undeclared action"
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
        json!({"command": "RESTART.NOW", "serviceName": "collectd", "serviceType": "service"}),
        "Invalid action name";
        "an action with a dot"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "RESTART", "serviceType": "service"}),
        "neither the operation nor the registration of the service gives a service name";
        "no service name"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "RESTART", "serviceName": "--now", "serviceType": "service"}),
        "Invalid service name";
        "a service name looking like an option"
    )]
    #[test_case(
        json!({"@type": "service", "name": "--now"}),
        json!({"command": "RESTART", "serviceType": "service"}),
        "Invalid service name";
        "a registered name looking like an option"
    )]
    #[test_case(
        json!({"@type": "service"}),
        json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "../../bin/sh"}),
        "Invalid service type";
        "a service type naming no plugin file"
    )]
    #[test_case(
        json!({"@type": "service", "type": "../../bin/sh"}),
        json!({"command": "RESTART", "serviceName": "collectd"}),
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
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                "{}",
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
                    "c8y_ServiceCommand": {"command": "RESTART", "serviceName": "collectd"},
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
                &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart"),
                "{}",
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
                            "command": "RESTART",
                            "serviceName": service_name,
                        },
                        "externalSource": {"externalId": SERVICE_XID, "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [(
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": service_name,
                    "serviceType": "service",
                })
                .into(),
            )],
        );
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
