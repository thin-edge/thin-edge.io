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
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::Topic;
use tracing::info;
use tracing::warn;

const INVENTORY_FRAGMENTS_FILE_LOCATION: &str = "device/inventory.json";
const INVENTORY_MANAGED_OBJECTS_TOPIC: &str = "c8y/inventory/managedObjects/update";

impl CumulocityConverter {
    /// Creates the inventory update message with fragments from inventory.json file
    /// while also updating the live `inventory_model` of this converter
    pub(crate) fn parse_base_inventory_file(&mut self) -> Result<Vec<Message>, ConversionError> {
        let mut messages = vec![];
        let inventory_file_path = self.cfg_dir.join(INVENTORY_FRAGMENTS_FILE_LOCATION);
        let mut inventory_base = Self::get_inventory_fragments(&inventory_file_path)?;

        if let Some(map) = inventory_base.as_object_mut() {
            if map.remove("name").is_some() {
                warn!("Ignoring `name` fragment key from inventory.json as updating the same using this file is not supported");
            }
            if map.remove("type").is_some() {
                warn!("Ignoring `type` fragment key from inventory.json as updating the same using this file is not supported");
            }
        }

        let message =
            self.inventory_update_message(&self.device_topic_id, inventory_base.clone())?;
        messages.push(message);

        if let JsonValue::Object(map) = inventory_base {
            for (key, value) in map {
                let main_device_tid = self.entity_store.main_device().clone();
                let _ = self.entity_store.update_twin_data(
                    &main_device_tid,
                    key.clone(),
                    value.clone(),
                )?;
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
    ) -> Message {
        let twin_channel = Channel::EntityTwinData { fragment_key };
        let topic = self.mqtt_schema.topic_for(entity, &twin_channel);
        Message::new(&topic, fragment_value.to_string()).with_retain()
    }

    /// Convert a twin metadata message into Cumulocity inventory update messages.
    /// Updating the `name` and `type` fragments are not supported.
    /// Empty payload is mapped to a clear inventory fragment message in Cumulocity.
    pub(crate) fn try_convert_entity_twin_data(
        &mut self,
        source: &EntityTopicId,
        message: &Message,
        fragment_key: &str,
    ) -> Result<Vec<Message>, ConversionError> {
        if fragment_key == "name" || fragment_key == "type" {
            warn!("Updating the entity `name` and `type` fields via the twin/ topic channel is not supported");
            return Ok(vec![]);
        }

        let fragment_value = if message.payload_bytes().is_empty() {
            JsonValue::Null
        } else {
            serde_json::from_slice::<JsonValue>(message.payload_bytes())?
        };

        let updated = self.entity_store.update_twin_data(
            source,
            fragment_key.into(),
            fragment_value.clone(),
        )?;
        if !updated {
            return Ok(vec![]);
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
    ) -> Result<Message, ConversionError> {
        let entity_external_id = self.entity_store.try_get(source)?.external_id.as_ref();
        let inventory_update_topic = Topic::new_unchecked(&format!(
            "{INVENTORY_MANAGED_OBJECTS_TOPIC}/{entity_external_id}"
        ));

        Ok(Message::new(
            &inventory_update_topic,
            fragment_value.to_string(),
        ))
    }

    /// Create the inventory update message to update the `type` of the main device
    pub(crate) fn inventory_device_type_update_message(&self) -> Result<Message, ConversionError> {
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
}

#[cfg(test)]
mod tests {
    use crate::converter::tests::create_c8y_converter;
    use serde_json::json;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
    use tedge_mqtt_ext::test_helpers::MessagePayloadMatcher;
    use tedge_mqtt_ext::Message;
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
            Message::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
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

        let twin_message = Message::new(
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
    async fn unquoted_string_value_invalid() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = Message::new(
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

        let twin_message = Message::new(
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

        let twin_message = Message::new(
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
    async fn forbidden_fragment_keys() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        let twin_message = Message::new(
            &Topic::new_unchecked("te/device/main///twin/name"),
            r#"New Name"#,
        );
        let inventory_messages = converter.convert(&twin_message).await;
        println!("{:?}", inventory_messages);
        assert!(
            inventory_messages.is_empty(),
            "Expected no converted messages, but received {:?}",
            &inventory_messages
        );
    }

    #[tokio::test]
    async fn clear_inventory_fragment() {
        let tmp_dir = TempTedgeDir::new();
        let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir).await;

        // Register a twin data fragment first
        let twin_message = Message::new(
            &Topic::new_unchecked("te/device/main///twin/foo"),
            "\"bar\"",
        );
        let _ = converter.convert(&twin_message).await;

        // Clear that fragment
        let twin_message = Message::new(&Topic::new_unchecked("te/device/main///twin/foo"), "");
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
            Message::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
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
        let twin_message = Message::new(
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
            Message::new(&Topic::new_unchecked(twin_topic), twin_payload.to_string());
        let inventory_messages = converter.convert(&twin_message).await;

        let expected_message: (&'static str, MessagePayloadMatcher) = (
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
            Message::new(&Topic::new_unchecked("te/device/main///twin/device_os"), "");
        let _ = converter.convert(&clear_message).await;

        // Assert duplicate payload converted after it was cleared
        let inventory_messages = converter.convert(&twin_message).await;
        assert_messages_matching(&inventory_messages, [expected_message]);
    }
}
