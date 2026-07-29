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
use serde_json::json;
use std::collections::BTreeSet;
use std::collections::HashMap;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::service_command::validate_action_name;
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

    const SMARTREST_TOPIC: &str = "c8y/s/us/test-device:device:main:service:collectd";
    const INVENTORY_TOPIC: &str =
        "c8y/inventory/managedObjects/update/test-device:device:main:service:collectd";

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
