use crate::config::ConfigError;
use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct SetTopic {
    topic: String,
}

impl Transformer for SetTopic {
    fn name(&self) -> &str {
        "set-topic"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        let Some(topic) = config.string_property("topic") else {
            return Err(ConfigError::IncorrectSetting(format!(
                "No topic configured for {} step",
                self.name()
            )));
        };
        self.topic = topic.to_owned();
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        let mut transformed_message = message.clone();
        transformed_message.topic = self.topic.clone();
        Ok(vec![transformed_message])
    }
}
