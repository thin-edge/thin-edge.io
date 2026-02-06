use crate::mea::entities::C8yEntityBirth;
use crate::mea::get_entity_metadata;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::store::RingBuffer;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;

/// Return the messages unchanged, unless related to an entity not registered yet.
///
/// - Cache messages from unregistered entity
/// - Return the cached messages when an entity birth message is received
#[derive(Clone, Default)]
pub struct MessageCache {
    mqtt_schema: MqttSchema,
    cache: RingBuffer<Message>,
}

impl tedge_flows::Transformer for MessageCache {
    fn name(&self) -> &str {
        "cache-early-messages"
    }

    fn set_config(&mut self, config: JsonValue) -> Result<(), ConfigError> {
        if let Some(root) = config.string_property("topic_root") {
            self.mqtt_schema = MqttSchema::with_root(root.to_string())
        }
        Ok(())
    }

    fn on_message(
        &mut self,
        _timestamp: SystemTime,
        message: &Message,
        context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((_, Channel::Status { component })) if component == "entities" => {
                let birth_message =
                    C8yEntityBirth::from_json(message.payload.as_slice()).map_err(|err| {
                        FlowError::UnsupportedMessage(format!(
                            "Invalid entity birth message received on {}: {err}",
                            message.topic
                        ))
                    })?;

                Ok(self.restore_messages(birth_message.entity_topic.as_str()))
            }

            Ok((entity_id, _)) => {
                if get_entity_metadata(context, entity_id.as_str()).is_none() {
                    self.cache.push(message.clone());
                    return Ok(vec![]);
                };

                // In case the current message has been received before the entity birth message
                // all the messages cached for that entity have to be processed first
                let mut messages = self.restore_messages(&message.topic);
                messages.push(message.clone());
                Ok(messages)
            }

            _ => Ok(vec![message.clone()]),
        }
    }
}

impl MessageCache {
    /// Retrieve from the cache all the messages related to the entity with the given topic
    pub fn restore_messages(&mut self, entity_topic: &str) -> Vec<Message> {
        let mut messages = vec![];
        let pending_messages = self.cache.take();
        for message in pending_messages.into_iter() {
            if message.topic.starts_with(entity_topic) {
                messages.push(message);
            } else {
                self.cache.push(message);
            }
        }
        messages
    }
}
