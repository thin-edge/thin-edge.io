use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;
use tedge_utils::timestamp::TimeFormat;
use time::OffsetDateTime;

#[derive(Default, Clone)]
pub struct AddTimestamp;

impl Transformer for AddTimestamp {
    fn name(&self) -> &str {
        "add-timestamp"
    }

    fn on_message(
        &self,
        time: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        let Ok(serde_json::Value::Object(json_message)) =
            serde_json::from_slice(message.payload.as_slice())
        else {
            return Ok(vec![message.clone()]);
        };

        let time_property = config.string_property("property").unwrap_or("time");
        let format_name = config.string_property("format").unwrap_or("unix");
        let reformat = config.bool_property("reformat").unwrap_or(false);
        let Ok(format) = TimeFormat::try_from(format_name) else {
            return Err(FlowError::IncorrectSetting(format!(
                "Unknown time format: {format_name}"
            )));
        };

        let mut json_message = json_message;

        let result = match json_message.get(time_property) {
            Some(_) if !reformat => return Ok(vec![message.clone()]),
            Some(timestamp) => format.reformat_json(timestamp.clone()),
            None => format.to_json(OffsetDateTime::from(time)),
        };

        let Ok(new_timestamp) = result else {
            return Err(FlowError::UnsupportedMessage(format!(
                "Failed to format message timestamp as {format_name}"
            )));
        };

        json_message.insert(time_property.to_owned(), new_timestamp);

        let transformed_topic = message.topic.to_owned();
        let transformed_payload = serde_json::Value::Object(json_message).to_string();
        Ok(vec![Message::new(transformed_topic, transformed_payload)])
    }
}
