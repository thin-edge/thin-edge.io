//! Mapping to Cumulocity the command capabilities of a service.
//!
//! A service tells which actions it supports with one `cmd/<action>` capability topic per action.
//! Cumulocity names them all in a single `c8y_SupportedServiceCommands` fragment of the service
//! managed object, and triggers any of them with a single `c8y_ServiceCommand` operation.
//!
//! So, unlike a command capability of a device, a command capability of a service is not mapped to
//! a Cumulocity operation of its own: a service declaring `cmd/restart` declares the service
//! command `RESTART`, not the device operation `c8y_Restart`.

use crate::converter::CumulocityConverter;
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
use tedge_api::service_command::validate_action_name;
use tedge_api::service_command::validate_service_name;
use tedge_api::service_command::validate_service_type;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::warn;

/// The Cumulocity operation triggering any action of a service.
pub const C8Y_SERVICE_COMMAND: &str = "c8y_ServiceCommand";

/// The inventory fragment listing the actions supported by a service.
const C8Y_SUPPORTED_SERVICE_COMMANDS: &str = "c8y_SupportedServiceCommands";

/// The actions declared by the services, in alphabetical order.
#[derive(Debug, Default)]
pub struct ServiceCommands {
    actions_by_service: HashMap<EntityTopicId, BTreeSet<String>>,
}

impl ServiceCommands {
    /// Add an action to those declared by a service.
    ///
    /// Return false if this action was already declared.
    fn declare(&mut self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .entry(service.clone())
            .or_default()
            .insert(action.to_string())
    }

    /// Remove an action from those declared by a service.
    ///
    /// Return false if this action was not declared.
    fn withdraw(&mut self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .get_mut(service)
            .is_some_and(|actions| actions.remove(action))
    }

    /// Forget every action declared by a service.
    ///
    /// Called when the entity is deregistered. Nothing is published: an empty set means a service
    /// which declares no action, not a service which is gone. A topic identifier can be registered
    /// again by another service, which must then be the only one to decide what it declares.
    pub(crate) fn forget(&mut self, service: &EntityTopicId) {
        self.actions_by_service.remove(service);
    }

    /// Tells whether an action is among those declared by a service.
    ///
    /// The action name is expected to be lowercase, as every declared name is. Lowercasing the
    /// command name sent by Cumulocity is what makes this comparison case-insensitive.
    fn declares(&self, service: &EntityTopicId, action: &str) -> bool {
        self.actions_by_service
            .get(service)
            .is_some_and(|actions| actions.contains(action))
    }

    /// The Cumulocity names of the actions declared by a service.
    ///
    /// Cumulocity uppercases a command name by convention.
    fn c8y_names(&self, service: &EntityTopicId) -> Vec<String> {
        self.actions_by_service
            .get(service)
            .map(|actions| actions.iter().map(|action| action.to_uppercase()).collect())
            .unwrap_or_default()
    }
}

impl CumulocityConverter {
    /// Convert the declaration, or the withdrawal, of a command capability of a service.
    ///
    /// The whole set of actions declared by that service is published on every change, because
    /// Cumulocity holds the set as a single inventory fragment. A service declaring an action also
    /// supports the `c8y_ServiceCommand` operation, which is how the cloud triggers an action.
    pub(crate) async fn convert_service_command_metadata(
        &mut self,
        service: &EntityTopicId,
        operation: &OperationType,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let action = operation.name();
        if let Err(err) = validate_action_name(&action) {
            warn!(
                topic = %message.topic.name,
                "Not declaring the action of a service to Cumulocity: {err}"
            );
            return Ok(vec![]);
        }

        let withdrawn = message.payload_bytes().is_empty();
        let changed = if withdrawn {
            self.service_commands.withdraw(service, &action)
        } else {
            self.service_commands.declare(service, &action)
        };
        if !changed {
            return Ok(vec![]);
        }

        let mut messages = vec![];

        if !withdrawn {
            // The operation directory of a service is not watched for changes, hence the operation
            // is registered here, on every newly declared action. `register_operation` tells the
            // cloud only when the set of supported operations actually changes, so this registers
            // `c8y_ServiceCommand` once per service.
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

    /// Convert a `c8y_ServiceCommand` operation into the thin-edge command of a service.
    ///
    /// The action to run is named by the topic of the published command and nowhere else.
    /// Cumulocity uppercases a command name by convention, so lowercasing the name it sends gives
    /// back the action name the service declared.
    ///
    /// No command is published when the operation cannot be honored. The cloud operation is failed
    /// instead, since no thin-edge command would ever report a status for it.
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

        let action = request.command.to_lowercase();
        if let Err(err) = validate_action_name(&action) {
            let reason = format!(
                "{} cannot run the command '{}': {err}",
                target_xid.as_ref(),
                request.command
            );
            return self.fail_service_command(&target_xid, target.r#type(), op_id, &reason);
        }
        if !self.service_commands.declares(target.topic_id(), &action) {
            let reason = format!(
                "{} has not declared the '{action}' action",
                target_xid.as_ref()
            );
            return self.fail_service_command(&target_xid, target.r#type(), op_id, &reason);
        }

        // The name of the service to act on, as the operation gives it. Cumulocity holds one name
        // per service, the very name thin-edge published when it registered the service, so this
        // is the name a backend is asked for. It is validated here rather than left to the backend,
        // so that a name a backend could misread is reported as the reason of the failed operation.
        let service_name = request.service_name.as_str();
        if let Err(err) = validate_service_name(service_name) {
            let reason = format!(
                "{} cannot run the '{action}' action: {err}",
                target_xid.as_ref()
            );
            return self.fail_service_command(&target_xid, target.r#type(), op_id, &reason);
        }

        // The type selects the backend, so what the service registered itself with wins. The value
        // Cumulocity sends is only used for a service registered with no type of its own.
        let service_type = target
            .registered_type()
            .or_else(|| Some(request.service_type.as_str()).filter(|ty| !ty.is_empty()))
            .unwrap_or(DEFAULT_SERVICE_TYPE);

        // Whichever of the two it comes from, the type names a file in the service plugin
        // directory, so it is validated for the same reason the name is
        if let Err(err) = validate_service_type(service_type) {
            let reason = format!(
                "{} cannot run the '{action}' action: {err}",
                target_xid.as_ref()
            );
            return self.fail_service_command(&target_xid, target.r#type(), op_id, &reason);
        }

        let topic = self.mqtt_schema.topic_for(
            target.topic_id(),
            &Channel::Command {
                operation: action.as_str().into(),
                cmd_id,
            },
        );
        let payload = json!({
            "serviceName": service_name,
            "serviceType": service_type,
        });
        let command = GenericCommandState::new(topic, CommandStatus::Init.to_string(), payload)
            .into_message();

        vec![command]
    }

    /// Tell Cumulocity that a service command failed, no command being published for it.
    ///
    /// The failure is reported on the SmartREST topic of the target, the very topic Cumulocity
    /// created the operation on. A target that could not be resolved is addressed as the service
    /// a service command always names: its external id is known even when nothing else about it
    /// is, and the main device owns no operation created for a service.
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
            // Unlike every other external id the mapper publishes on, this one is not pre-validated:
            // it is whatever the cloud addressed the operation to
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
    use crate::converter::CumulocityConverter;
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

    /// The Cumulocity id of the operation triggered by the tests, and the command topic it gives
    const OPERATION_ID: &str = "16574089";
    const RESTART_CMD_TOPIC: &str =
        "te/device/main/service/collectd/cmd/restart/c8y-mapper-16574089";

    #[tokio::test]
    async fn declared_actions_are_published_in_uppercase() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        // The first action of a service makes it support the `c8y_ServiceCommand` operation
        assert_messages_matching(
            &declare(&mut converter, "start").await,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
                ),
            ],
        );

        // The whole set is published again on every new action
        assert_messages_matching(
            &declare(&mut converter, "stop").await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["START", "STOP"]}).into(),
            )],
        );

        assert_messages_matching(
            &declare(&mut converter, "restart").await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["RESTART", "START", "STOP"]}).into(),
            )],
        );

        // A capability declared twice changes nothing
        assert!(declare(&mut converter, "restart").await.is_empty());
    }

    #[tokio::test]
    async fn a_deregistered_service_leaves_no_action_behind() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        declare(&mut converter, "start").await;
        declare(&mut converter, "stop").await;

        deregister_service(&mut converter).await;
        register_typed_service(&mut converter, json!({})).await;

        // A service registered again under the same topic identifier is the only one to decide
        // what it declares: the actions of the previous one are gone
        assert_messages_matching(
            &declare(&mut converter, "start").await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn a_custom_action_is_passed_through() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        let messages = declare(&mut converter, "collect_measurements").await;

        assert_messages_matching(
            &messages,
            [
                (SMARTREST_TOPIC, "114,c8y_ServiceCommand".into()),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["COLLECT_MEASUREMENTS"]}).into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn a_name_which_is_not_an_action_name_is_not_declared() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        // Such an action could not be triggered from the cloud anyway:
        // lowercasing the name Cumulocity sends back would not give this name
        assert!(declare(&mut converter, "doSomething").await.is_empty());
    }

    #[tokio::test]
    async fn a_withdrawn_action_is_removed_from_the_published_set() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        declare(&mut converter, "start").await;
        declare(&mut converter, "pause").await;

        // The supported operation is left registered: the service still has actions
        assert_messages_matching(
            &withdraw(&mut converter, "pause").await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
            )],
        );

        // An action which was never declared is not published again
        assert!(withdraw(&mut converter, "pause").await.is_empty());
    }

    #[tokio::test]
    async fn withdrawing_every_action_publishes_an_empty_set() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        declare(&mut converter, "start").await;
        declare(&mut converter, "stop").await;
        withdraw(&mut converter, "start").await;

        // Cumulocity then offers no action for that service
        assert_messages_matching(
            &withdraw(&mut converter, "stop").await,
            [(
                INVENTORY_TOPIC,
                json!({"c8y_SupportedServiceCommands": []}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn the_set_is_rebuilt_from_the_retained_capabilities_after_a_restart() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        for action in ["start", "stop", "restart"] {
            declare(&mut converter, action).await;
        }

        // A restarted mapper receives the retained capability messages again
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;

        let mut published = vec![];
        for action in ["start", "stop", "restart"] {
            published.append(&mut declare(&mut converter, action).await);
        }

        // No action is lost, and the supported operation is not announced again
        assert_messages_matching(
            &published,
            [
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["START"]}).into(),
                ),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["START", "STOP"]}).into(),
                ),
                (
                    INVENTORY_TOPIC,
                    json!({"c8y_SupportedServiceCommands": ["RESTART", "START", "STOP"]}).into(),
                ),
            ],
        );
    }

    #[tokio::test]
    async fn a_declared_action_becomes_a_command_of_the_service() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        // The action is the lowercased command name, and it is named by the topic only
        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "RESTART", "serviceName": "collectd", "serviceType": ""}),
            )
            .await,
            [(
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn an_action_named_with_a_dash_survives_the_round_trip() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "is-active").await;

        // The name is declared uppercased and comes back uppercased, and `-` survives both
        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "IS-ACTIVE", "serviceName": "collectd", "serviceType": ""}),
            )
            .await,
            [(
                "te/device/main/service/collectd/cmd/is-active/c8y-mapper-16574089",
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn the_type_of_the_registration_wins_over_the_type_of_the_payload() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_typed_service(&mut converter, json!({"type": "container"})).await;
        declare(&mut converter, "restart").await;

        // The type selects the backend running the action, so the local truth wins
        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "systemd"}),
            )
            .await,
            [(
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": "container",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn the_type_of_the_payload_is_used_when_the_service_registered_none() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "container"}),
            )
            .await,
            [(
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": "collectd",
                    "serviceType": "container",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn the_name_of_the_service_comes_from_the_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_typed_service(&mut converter, json!({"name": "Collectd-Daemon"})).await;
        declare(&mut converter, "restart").await;

        // The name given at registration is the name Cumulocity holds for that service, and the
        // one it sends back. The target is resolved from the external id, not from that name, so
        // the command still goes to the topic of the registered service
        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "RESTART", "serviceName": "Collectd-Daemon", "serviceType": ""}),
            )
            .await,
            [(
                RESTART_CMD_TOPIC,
                json!({
                    "status": "init",
                    "serviceName": "Collectd-Daemon",
                    "serviceType": "service",
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn an_unresolvable_target_fails_the_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        // Nothing else is known of that target, but its external id names the topic Cumulocity
        // created the operation on
        let messages = trigger_on(
            &mut converter,
            "unknown-service",
            json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "service"}),
        )
        .await;

        assert_operation_failed(&messages, "c8y/s/us/unknown-service", "unknown-service");
    }

    #[tokio::test]
    async fn an_operation_addressed_to_the_main_device_is_failed_on_its_own_topic() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        // A device declares no service action, so such an operation is refused. It is reported
        // where Cumulocity created it, and the main device's own topic carries no external id
        let messages = trigger_on(
            &mut converter,
            "test-device",
            json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "service"}),
        )
        .await;

        assert_operation_failed(&messages, "c8y/s/us", "has not declared");
    }

    #[tokio::test]
    async fn an_undeclared_action_fails_the_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "start").await;

        let messages = trigger(
            &mut converter,
            json!({"command": "RESTART", "serviceName": "collectd", "serviceType": "service"}),
        )
        .await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, "has not declared the 'restart'");
    }

    #[test_case(json!({"command": "", "serviceName": "collectd", "serviceType": "service"}); "empty")]
    #[test_case(json!({"command": "do something", "serviceName": "collectd", "serviceType": "service"}); "with a space")]
    #[test_case(json!({"command": "RESTART.NOW", "serviceName": "collectd", "serviceType": "service"}); "with a dot")]
    #[tokio::test]
    async fn a_command_naming_no_action_fails_the_operation(request: serde_json::Value) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        // Lowercasing such a name gives no action name, so it names no capability topic either
        let messages = trigger(&mut converter, request).await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, "Invalid action name");
    }

    #[test_case(json!({"command": "RESTART", "serviceName": "", "serviceType": "service"}); "empty")]
    #[test_case(json!({"command": "RESTART", "serviceName": "--now", "serviceType": "service"}); "looking like an option")]
    #[tokio::test]
    async fn a_name_no_backend_can_be_asked_for_fails_the_operation(request: serde_json::Value) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        let messages = trigger(&mut converter, request).await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, "Invalid service name");
    }

    #[test_case("dbus-:1.2-org.freedesktop.problems@0"; "a systemd unit name holding a colon")]
    #[test_case("Nginx Web Server"; "a display name holding spaces")]
    #[test_case("collectd;reboot"; "holding a shell separator")]
    #[tokio::test]
    async fn a_name_the_device_registered_reaches_the_command(service_name: &str) {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_typed_service(&mut converter, json!({ "name": service_name })).await;
        declare(&mut converter, "restart").await;

        assert_messages_matching(
            &trigger(
                &mut converter,
                json!({"command": "RESTART", "serviceName": service_name, "serviceType": ""}),
            )
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

    #[tokio::test]
    async fn a_type_naming_no_plugin_file_fails_the_operation() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_service(&mut converter).await;
        declare(&mut converter, "restart").await;

        let messages = trigger(
            &mut converter,
            json!({
                "command": "RESTART",
                "serviceName": "collectd",
                "serviceType": "../../bin/sh",
            }),
        )
        .await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, "Invalid service type");
    }

    #[tokio::test]
    async fn a_type_naming_no_plugin_file_is_refused_from_the_registration_too() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
        register_typed_service(&mut converter, json!({"type": "../../bin/sh"})).await;
        declare(&mut converter, "restart").await;

        // The registered type wins over the payload, so it is the one to check
        let messages = trigger(
            &mut converter,
            json!({
                "command": "RESTART",
                "serviceName": "collectd",
                "serviceType": "container",
            }),
        )
        .await;

        assert_operation_failed(&messages, SMARTREST_TOPIC, "Invalid service type");
    }

    async fn register_service(converter: &mut CumulocityConverter) {
        register_source_entities("te/device/main/service/collectd/cmd/restart", converter).await
    }

    /// Deregister the service, as `DELETE /te/v1/entities/<topic-id>` does
    async fn deregister_service(converter: &mut CumulocityConverter) {
        let topic = Topic::new_unchecked("te/device/main/service/collectd");
        converter
            .process_entity_metadata_message(&MqttMessage::new(&topic, ""))
            .await
            .unwrap();
    }

    /// Register the service with the given registration data, as a service declaring itself does
    async fn register_typed_service(
        converter: &mut CumulocityConverter,
        registration: serde_json::Value,
    ) {
        register_source_entities("te/device/main///", converter).await;

        let mut registration = registration;
        registration["@type"] = json!("service");
        registration["@parent"] = json!("device/main//");

        let topic = Topic::new_unchecked("te/device/main/service/collectd");
        converter
            .process_entity_metadata_message(&MqttMessage::new(&topic, registration.to_string()))
            .await
            .unwrap();
    }

    /// Feed the converter a `c8y_ServiceCommand` operation addressed to the collectd service
    async fn trigger(
        converter: &mut CumulocityConverter,
        request: serde_json::Value,
    ) -> Vec<MqttMessage> {
        trigger_on(converter, SERVICE_XID, request).await
    }

    async fn trigger_on(
        converter: &mut CumulocityConverter,
        target_xid: &str,
        request: serde_json::Value,
    ) -> Vec<MqttMessage> {
        let operation = json!({
            "id": OPERATION_ID,
            "status": "PENDING",
            "c8y_ServiceCommand": request,
            "externalSource": {"externalId": target_xid, "type": "c8y_Serial"},
        });

        let topic = Topic::new_unchecked("c8y/devicecontrol/notifications");
        converter
            .convert(&MqttMessage::new(&topic, operation.to_string()))
            .await
    }

    /// Check that the operation is failed to Cumulocity, no command being published for it
    fn assert_operation_failed(messages: &[MqttMessage], topic: &str, reason: &str) {
        let payloads: Vec<&str> = messages
            .iter()
            .inspect(|message| assert_eq!(message.topic.name, topic))
            .map(|message| message.payload_str().unwrap())
            .collect();

        assert_eq!(
            payloads.len(),
            2,
            "expecting only 501 and 502: {payloads:?}"
        );
        assert_eq!(payloads[0], "501,c8y_ServiceCommand");
        assert!(
            payloads[1].starts_with("502,c8y_ServiceCommand,"),
            "not a failure of the service command operation: {}",
            payloads[1]
        );
        assert!(
            payloads[1].contains(reason),
            "the reason does not tell about '{reason}': {}",
            payloads[1]
        );
    }

    async fn declare(converter: &mut CumulocityConverter, action: &str) -> Vec<MqttMessage> {
        capability(converter, action, "{}").await
    }

    async fn withdraw(converter: &mut CumulocityConverter, action: &str) -> Vec<MqttMessage> {
        capability(converter, action, "").await
    }

    async fn capability(
        converter: &mut CumulocityConverter,
        action: &str,
        payload: &str,
    ) -> Vec<MqttMessage> {
        let topic = Topic::new_unchecked(&format!("te/device/main/service/collectd/cmd/{action}"));
        converter.convert(&MqttMessage::new(&topic, payload)).await
    }
}
