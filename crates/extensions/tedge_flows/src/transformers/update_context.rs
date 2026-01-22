use crate::ConfigError;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::JsonValue;
use crate::Message;
use crate::Transformer;
use std::time::SystemTime;
use tedge_mqtt_ext::TopicFilter;

#[derive(Clone)]
pub struct UpdateContext {
    topics: TopicFilter,
}

impl Default for UpdateContext {
    fn default() -> Self {
        UpdateContext {
            topics: TopicFilter::new_unchecked("#"),
        }
    }
}

impl Transformer for UpdateContext {
    fn name(&self) -> &str {
        "update-context"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        let topics = config.strings_property("topics");
        self.topics = crate::config::topic_filters(topics)?;
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        if self.topics.accept_topic_name(&message.topic) {
            if message.payload.is_empty() {
                context.set_value(&message.topic, JsonValue::Null);
                return Ok(vec![]);
            }
            let json_message: serde_json::Value =
                serde_json::from_slice(message.payload.as_slice()).map_err(|_| {
                    FlowError::UnsupportedMessage(
                        "Failed to update the context: not a JSON payload".to_string(),
                    )
                })?;

            context.set_value(&message.topic, json_message.into());
            Ok(vec![])
        } else {
            Ok(vec![message.clone()])
        }
    }
}
