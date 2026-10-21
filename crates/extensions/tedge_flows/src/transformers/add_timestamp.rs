use crate::config::ConfigError;
use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;
use tedge_utils::timestamp::TimeFormat;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct AddTimestamp {
    time_property: String,
    format: TimeFormat,
    reformat: bool,
}

impl Default for AddTimestamp {
    fn default() -> Self {
        AddTimestamp {
            time_property: "time".to_string(),
            format: TimeFormat::Unix,
            reformat: false,
        }
    }
}

impl Transformer for AddTimestamp {
    fn name(&self) -> &str {
        "add-timestamp"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        if let Some(time_property) = config.string_property("property") {
            self.time_property = time_property.to_owned();
        }
        if let Some(format_name) = config.string_property("format") {
            let Ok(format) = format_name.parse::<TimeFormat>() else {
                return Err(ConfigError::IncorrectSetting(format!(
                    "Unknown time format: {format_name}"
                )));
            };
            self.format = format;
        }
        if let Some(reformat) = config.bool_property("reformat") {
            self.reformat = reformat;
        }

        Ok(())
    }

    fn on_message(
        &mut self,
        time: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let Ok(serde_json::Value::Object(json_message)) =
            serde_json::from_slice(message.payload.as_slice())
        else {
            return Ok(vec![message.clone()]);
        };

        let mut json_message = json_message;

        let result = match json_message.get(&self.time_property) {
            Some(_) if !self.reformat => return Ok(vec![message.clone()]),
            Some(timestamp) => self.format.reformat_json(timestamp.clone()),
            None => self.format.to_json(OffsetDateTime::from(time)),
        };

        let Ok(new_timestamp) = result else {
            return Err(FlowError::UnsupportedMessage(format!(
                "Failed to format message timestamp as {}",
                self.format
            )));
        };

        json_message.insert(self.time_property.to_owned(), new_timestamp);

        let transformed_topic = message.topic.to_owned();
        let transformed_payload = serde_json::Value::Object(json_message).to_string();
        let mut transformed_message = Message::new(transformed_topic, transformed_payload);
        // Keep the file where the message is written, when the flow output is a directory
        transformed_message.file = message.file.clone();
        Ok(vec![transformed_message])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageFile;

    #[test]
    fn keeps_the_file_of_a_transformed_message() {
        let mut step = AddTimestamp::default();
        let mut message = Message::new("te/device/main///m/env", r#"{"temperature": 23}"#);
        message.file = Some(MessageFile {
            name: "date=2026-09-14/env.jsonl".to_string(),
        });

        let output = step
            .on_message(SystemTime::now(), &message, &FlowContextHandle::default())
            .unwrap();

        assert_eq!(output.len(), 1);
        assert!(output[0].payload_str().unwrap().contains("\"time\""));
        assert_eq!(output[0].file, message.file);
    }
}
