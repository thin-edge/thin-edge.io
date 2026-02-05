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
                return self.cached_messages(message);
            }

            Ok((entity_id, _)) => {
                if get_entity_metadata(context, entity_id.as_str()).is_none() {
                    self.cache.push(message.clone());
                    return Ok(vec![]);
                };
            }

            _ => (),
        }

        Ok(vec![message.clone()])
    }
}

impl MessageCache {
    pub fn cached_messages(&mut self, message: &Message) -> Result<Vec<Message>, FlowError> {
        let birth_message =
            C8yEntityBirth::from_json(message.payload.as_slice()).map_err(|err| {
                FlowError::UnsupportedMessage(format!(
                    "Invalid entity birth message received on {}: {err}",
                    message.topic
                ))
            })?;

        let mut messages = vec![];
        let pending_messages = self.cache.take();
        for message in pending_messages.into_iter() {
            if birth_message.matches_entity(&message.topic) {
                messages.push(message);
            } else {
                self.cache.push(message);
            }
        }

        Ok(messages)
    }
}
