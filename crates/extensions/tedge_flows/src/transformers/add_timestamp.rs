use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Default, Clone)]
pub struct AddTimestamp;

impl Transformer for AddTimestamp {
    fn name(&self) -> &str {
        "add-timestamp"
    }

    fn on_message(
        &self,
        timestamp: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        let Ok(serde_json::Value::Object(json_message)) =
            serde_json::from_slice(message.payload.as_slice())
        else {
            return Ok(vec![message.clone()]);
        };

        let time_property = config.string_property("property").unwrap_or("time");
        if json_message.get(time_property).is_some() {
            return Ok(vec![message.clone()]);
        }

        let mut json_message = json_message;
        json_message.insert(
            time_property.to_owned(),
            timestamp
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .into(),
        );
        let transformed_topic = message.topic.to_owned();
        let transformed_payload = serde_json::Value::Object(json_message).to_string();
        Ok(vec![Message::new(transformed_topic, transformed_payload)])
    }
}
