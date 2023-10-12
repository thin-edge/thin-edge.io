use crate::converter::CumulocityConverter;
use crate::converter::INVENTORY_MANAGED_OBJECTS_TOPIC;
use crate::error::ConversionError;
use serde_json::json;
use serde_json::Value as JsonValue;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::Topic;
use tracing::warn;

impl CumulocityConverter {
    pub fn try_convert_entity_twin_data(
        &mut self,
        source: &EntityTopicId,
        message: &Message,
        fragment_key: &str,
    ) -> Result<Vec<Message>, ConversionError> {
        let target_entity = self.entity_store.try_get(source)?;
        let entity_external_id = target_entity.external_id.as_ref();

        if fragment_key == "name" || fragment_key == "type" {
            warn!("Updating the entity `name` and `type` fields via the twin/ topic channel is not supported");
            return Ok(vec![]);
        }

        let payload = serde_json::from_slice::<JsonValue>(message.payload_bytes())?;

        let mapped_json = json!({ fragment_key: payload });

        let topic = Topic::new_unchecked(&format!(
            "{INVENTORY_MANAGED_OBJECTS_TOPIC}/{entity_external_id}"
        ));
        Ok(vec![Message::new(&topic, mapped_json.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use crate::converter::tests::create_c8y_converter;
    use serde_json::json;
    use tedge_mqtt_ext::test_helpers::assert_messages_matching;
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
        assert_messages_matching(&messages, [("tedge/errors", "expected value".into())])
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
        assert!(inventory_messages.is_empty(), "No mapped messages expected");
    }
}
