use crate::ConfigError;
use crate::FlowContextHandle;
use crate::FlowError;
use crate::JsonValue;
use crate::Message;
use crate::Transformer;
use std::time::SystemTime;
use tedge_mqtt_ext::TopicFilter;

#[derive(Clone, Default)]
pub struct IgnoreTopics {
    topics: TopicFilter,
}

impl Transformer for IgnoreTopics {
    fn name(&self) -> &str {
        "ignore-topics"
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
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        if self.topics.accept_topic_name(&message.topic) {
            Ok(vec![])
        } else {
            Ok(vec![message.clone()])
        }
    }
}
