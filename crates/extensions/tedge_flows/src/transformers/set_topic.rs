use crate::js_value::JsonValue;
use crate::transformers::Transformer;
use crate::FlowError;
use crate::Message;
use std::time::SystemTime;

#[derive(Clone, Default)]
pub struct SetTopic;

impl Transformer for SetTopic {
    fn name(&self) -> &str {
        "set-topic"
    }

    fn on_message(
        &self,
        _timestamp: SystemTime,
        message: &Message,
        config: &JsonValue,
    ) -> Result<Vec<Message>, FlowError> {
        let Some(topic) = config.string_property("topic") else {
            return Err(FlowError::IncorrectSetting(format!(
                "No topic configured for {} step",
                self.name()
            )));
        };
        let mut transformed_message = message.clone();
        transformed_message.topic = topic.to_owned();
        Ok(vec![transformed_message])
    }
}
