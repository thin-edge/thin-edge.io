//! This module provides converter functions to update Cumulocity inventory with entity twin data.
use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::fragments::C8yAgentFragment;
use crate::fragments::C8yDeviceDataFragment;
use serde_json::json;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tedge_api::entity::EntityType;
use tedge_api::entity_store::EntityTwinMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::info;

const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "inventory/managedObjects/update";

impl CumulocityConverter {
    /// Creates the inventory update message with fragments from inventory.json file
    /// while also updating the live `inventory_model` of this converter
    pub(crate) fn parse_base_inventory_file(
        &mut self,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let mut messages = vec![];
        let inventory_file_path = self
            .config
            .config_dir
            .join(INVENTORY_FRAGMENTS_FILE_LOCATION);
        let inventory_base = Self::get_inventory_fragments(inventory_file_path.as_std_path())?;

        let message =
            self.inventory_update_message(&self.device_topic_id, inventory_base.clone())?;
        messages.push(message);

        if let JsonValue::Object(map) = inventory_base {
            for (key, value) in map {
                let main_device_tid = self.entity_cache.main_device_topic_id().clone();
                let _ = self.entity_cache.update_twin_data(EntityTwinMessage::new(
                    main_device_tid.clone(),
                    key.clone(),
                    value.clone(),
                ))?;
                let mapped_message =
                    self.entity_twin_data_message(&main_device_tid, key.clone(), value.clone());
                messages.push(mapped_message);
            }
        }

        Ok(messages)
    }

    /// Create an entity twin data message with the provided fragment
    fn entity_twin_data_message(
        &self,
        entity: &EntityTopicId,
        fragment_key: String,
        fragment_value: JsonValue,
    ) -> MqttMessage {
        let twin_channel = Channel::EntityTwinData { fragment_key };
        let topic = self.mqtt_schema.topic_for(entity, &twin_channel);
        MqttMessage::new(&topic, fragment_value.to_string()).with_retain()
    }

    /// Convert a twin metadata message into Cumulocity inventory update messages.
    /// Updating the `name` and `type` fragments are not supported.
    /// Empty payload is mapped to a clear inventory fragment message in Cumulocity.
    pub(crate) fn try_convert_entity_twin_data(
        &mut self,
        source: &EntityTopicId,
        entity_type: &EntityType,
        message: &MqttMessage,
        fragment_key: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let fragment_value = if message.payload_bytes().is_empty() {
            JsonValue::Null
        } else {
            serde_json::from_slice::<JsonValue>(message.payload_bytes())?
        };

        self.try_convert_twin_fragment(source, entity_type, fragment_key, &fragment_value)
    }

    pub(crate) fn try_convert_twin_fragment(
        &mut self,
        source: &EntityTopicId,
        entity_type: &EntityType,
        fragment_key: &str,
        fragment_value: &JsonValue,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let updated = self.entity_cache.update_twin_data(EntityTwinMessage::new(
            source.clone(),
            fragment_key.into(),
            fragment_value.clone(),
        ))?;
        if !updated {
            return Ok(vec![]);
        }

        self.convert_twin_fragment(source, entity_type, fragment_key, fragment_value)
    }

    pub(crate) fn convert_twin_fragment(
        &mut self,
        source: &EntityTopicId,
        entity_type: &EntityType,
        mut fragment_key: &str,
        fragment_value: &JsonValue,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if fragment_key == "firmware" {
            fragment_key = "c8y_Firmware";
        }

        // All services in C8Y must have a fixed `type` fragment called `c8y_Service`.
        // The service specific type fragment is called `serviceType` and hence
        // we need to map the entity `type` into `serviceType` for services.
        if entity_type == &EntityType::Service && fragment_key == "type" {
            fragment_key = "serviceType";
        }

        let mapped_json = json!({ fragment_key: fragment_value });
        let mapped_message = self.inventory_update_message(source, mapped_json)?;
        Ok(vec![mapped_message])
    }

    /// Create a Cumulocity inventory update message from a JSON fragment
    fn inventory_update_message(
        &self,
        source: &EntityTopicId,
        fragment_value: JsonValue,
    ) -> Result<MqttMessage, ConversionError> {
        let inventory_update_topic = self.get_inventory_update_topic(source)?;

        Ok(MqttMessage::new(
            &inventory_update_topic,
            fragment_value.to_string(),
        ))
    }

    /// Create the inventory update message to update the `type` of the main device
    pub(crate) fn inventory_device_type_update_message(
        &self,
    ) -> Result<MqttMessage, ConversionError> {
        let device_data = C8yDeviceDataFragment::from_type(&self.device_type)?;
        let device_type_fragment = device_data.to_json()?;

        self.inventory_update_message(&self.device_topic_id, device_type_fragment)
    }

    /// Return the contents of inventory.json file as a `JsonValue`
    fn get_inventory_fragments(inventory_file_path: &Path) -> Result<JsonValue, ConversionError> {
        let agent_fragment = C8yAgentFragment::new()?;
        let json_fragment = agent_fragment.to_json()?;

        match Self::read_json_from_file(inventory_file_path) {
            Ok(mut json) => {
                json.as_object_mut()
                    .ok_or(ConversionError::FromOptionError)?
                    .insert(
                        "c8y_Agent".to_string(),
                        json_fragment
                            .get("c8y_Agent")
                            .ok_or(ConversionError::FromOptionError)?
                            .to_owned(),
                    );
                Ok(json)
            }
            Err(ConversionError::FromStdIo(_)) => {
                info!("Could not read inventory fragments from file {inventory_file_path:?}");
                Ok(json_fragment)
            }
            Err(ConversionError::FromSerdeJson(e)) => {
                info!("Could not parse the {inventory_file_path:?} file due to: {e}");
                Ok(json_fragment)
            }
            Err(_) => Ok(json_fragment),
        }
    }

    /// reads a json file to serde_json::Value
    fn read_json_from_file(file_path: &Path) -> Result<serde_json::Value, ConversionError> {
        let mut file = File::open(Path::new(file_path))?;
        let mut data = String::new();
        file.read_to_string(&mut data)?;
        let json: serde_json::Value = serde_json::from_str(&data)?;
        info!("Read the fragments from {file_path:?} file");
        Ok(json)
    }

    /// Returns the JSON over MQTT inventory update topic
    pub fn get_inventory_update_topic(
        &self,
        source: &EntityTopicId,
    ) -> Result<Topic, ConversionError> {
        let entity_external_id = self.entity_cache.try_get(source)?.external_id.as_ref();
        Ok(Topic::new_unchecked(&format!(
            "{prefix}/{INVENTORY_MANAGED_OBJECTS_TOPIC}/{entity_external_id}",
            prefix = self.config.bridge_config.c8y_prefix,
        )))
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

    #[tokio::test]
    async fn convert_entity_twin_data_json_object() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_topic = "te/device/main///twin/device_os";
        let twin_payload = json!({
          "family": "Debian",
          "version": "11"
        });
        let twin_message =
            MqttMessage::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "device_os": {
                        "family": "Debian",
                        "version": "11"
                    }
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_entity_twin_data_string_value() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/foo"),
            r#""bar""#, // String values must be quoted to be valid JSON string values
        );
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "foo": "bar"
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn duplicate_twin_name_and_type_updates_ignored_after_registration() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Register a child with a name and type upfront
        let reg_message = &MqttMessage::new(
            &Topic::new_unchecked("te/device/child01//"),
            r#"{"@type": "child-device", "@id": "child01", "name": "child01", "type": "Rpi"}"#,
        );
        let _ = converter
            .try_register_source_entities(reg_message)
            .await
            .unwrap();

        // Re-send the same name as a twin update
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child01///twin/name"),
            r#""child01""#,
        );
        let inventory_messages = converter.convert(&twin_message).await;
        // Assert that the duplicate name update is ignored
        assert_messages_matching(&inventory_messages, []);

        // Re-send the same type as a twin update
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child01///twin/type"),
            r#""Rpi""#,
        );
        // Assert that the duplicate type update is ignored
        let inventory_messages = converter.convert(&twin_message).await;
        assert_messages_matching(&inventory_messages, []);

        // Update with a different name
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child01///twin/name"),
            r#""my_child01""#,
        );
        let inventory_messages = converter.convert(&twin_message).await;
        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/child01",
                json!({
                    "name": "my_child01"
                })
                .into(),
            )],
        );

        // Update with a different type
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child01///twin/type"),
            r#""Rpi4""#,
        );
        let inventory_messages = converter.convert(&twin_message).await;
        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/child01",
                json!({
                    "type": "Rpi4"
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn unquoted_string_value_invalid() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/foo"),
            "unquoted value",
        );
        let messages = converter.convert(&twin_message).await;
        assert_messages_matching(&messages, [("te/errors", "expected value".into())])
    }

    #[tokio::test]
    async fn convert_entity_twin_data_numeric_value() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/foo"),
            r#"5.6789"#,
        );
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "foo": 5.6789
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_entity_twin_data_boolean_value() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/enabled"),
            r#"false"#,
        );
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "enabled": false
                })
                .into(),
            )],
        );
    }

    #[tokio::test]
    async fn clear_inventory_fragment() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Register a twin data fragment first
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/foo"),
            "\"bar\"",
        );
        let _ = converter.convert(&twin_message).await;

        // Clear that fragment
        let twin_message = MqttMessage::new(&Topic::new_unchecked("te/device/main///twin/foo"), "");
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({ "foo": null }).into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_entity_twin_data_ignores_duplicate_fragment() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_topic = "te/device/main///twin/device_os";
        let twin_payload = json!({
          "family": "Debian",
          "version": "11"
        });
        let twin_message =
            MqttMessage::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "device_os": {
                        "family": "Debian",
                        "version": "11"
                    }
                })
                .into(),
            )],
        );

        // Assert duplicated payload not converted
        let inventory_messages = converter.convert(&twin_message).await;
        assert!(
            inventory_messages.is_empty(),
            "Expected no converted messages, but received {:?}",
            &inventory_messages
        );

        // Assert that the same payload with different key order is also ignored
        let twin_message = MqttMessage::new(
            &Topic::new_unchecked(twin_topic),
            r#"{"version": "11","family": "Debian"}"#,
        );
        let inventory_messages = converter.convert(&twin_message).await;
        assert!(
            inventory_messages.is_empty(),
            "Expected no converted messages, but received {:?}",
            &inventory_messages
        );
    }

    #[tokio::test]
    async fn convert_entity_twin_data_with_duplicate_fragment_after_clearing_it() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_topic = "te/device/main///twin/device_os";
        let twin_payload = json!({
          "family": "Debian",
          "version": "11"
        });
        let twin_message =
            MqttMessage::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
        let inventory_messages = converter.convert(&twin_message).await;

        let expected_message = (
            "c8y/inventory/managedObjects/update/test-device",
            json!({
                "device_os": {
                    "family": "Debian",
                    "version": "11"
                }
            })
            .into(),
        );
        assert_messages_matching(&inventory_messages, [expected_message.clone()]);

        let clear_message =
            MqttMessage::new(&Topic::new_unchecked("te/device/main///twin/device_os"), "");
        let _ = converter.convert(&clear_message).await;

        // Assert duplicate payload converted after it was cleared
        let inventory_messages = converter.convert(&twin_message).await;
        assert_messages_matching(&inventory_messages, [expected_message]);
    }

    #[tokio::test]
    async fn convert_entity_twin_data_with_firmware_update_for_main_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///twin/firmware"),
            r#"{"name":"firmware", "version":"1.0"}"#,
        );

        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device",
                json!({"c8y_Firmware":{"name":"firmware","version":"1.0"}}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_entity_twin_data_with_firmware_update_for_child_device() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///twin/firmware"),
            r#"{"name":"firmware", "version":"1.0"}"#,
        );

        register_source_entities(&twin_message.topic.name, &mut converter).await;

        converter
            .try_register_source_entities(&twin_message)
            .await
            .unwrap();

        let inventory_messages = converter.convert(&twin_message).await;

        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/test-device:device:child1",
                json!({"c8y_Firmware":{"name":"firmware","version":"1.0"}}).into(),
            )],
        );
    }

    #[tokio::test]
    async fn convert_service_type() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let reg_message = &MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/service01"),
            r#"{"@type": "service", "@id": "service01"}"#,
        );
        let _ = converter
            .try_register_source_entities(reg_message)
            .await
            .unwrap();

        let twin_message = MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/service01/twin/type"),
            r#""systemd""#,
        );
        let inventory_messages = converter.convert(&twin_message).await;

        // Assert that the `type` fragment is mapped to `serviceType`
        assert_messages_matching(
            &inventory_messages,
            [(
                "c8y/inventory/managedObjects/update/service01",
                json!({
                    "serviceType": "systemd"
                })
                .into(),
            )],
        );
    }
}
