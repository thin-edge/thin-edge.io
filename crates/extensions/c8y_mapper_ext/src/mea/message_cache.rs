use crate::mea::entities::C8yEntityBirth;
use crate::mea::get_entity_metadata;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::SystemTime;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
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
#[derive(Clone)]
pub struct MessageCache {
    mqtt_schema: MqttSchema,
    mapper_topic_id: EntityTopicId,
    cache: HashMap<EntityTopicId, RingBuffer<Message>>,
    birthed: HashSet<EntityTopicId>,
}

impl MessageCache {
    pub fn new(mapper_topic_id: EntityTopicId) -> Self {
        MessageCache {
            mqtt_schema: MqttSchema::default(),
            mapper_topic_id,
            cache: HashMap::default(),
            birthed: HashSet::default(),
        }
    }
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
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((source, Channel::Status { component }))
                if component == "entities" && source == self.mapper_topic_id =>
            {
                let birth_message =
                    C8yEntityBirth::from_json(message.payload.as_slice()).map_err(|err| {
                        FlowError::UnsupportedMessage(format!(
                            "Invalid entity birth message received on {}: {err}",
                            message.topic
                        ))
                    })?;

                Ok(self.restore_messages(&birth_message.entity))
            }

            Ok((entity_id, _)) if entity_id.is_default_main_device() => Ok(vec![message.clone()]),

            Ok((entity_id, _)) if !self.birthed.contains(&entity_id) => {
                self.cache_message(entity_id, message.clone());
                return Ok(vec![]);
            }

            _ => Ok(vec![message.clone()]),
        }
    }
}

impl MessageCache {
    /// Cache a messages for an entity
    pub fn cache_message(&mut self, entity_id: EntityTopicId, message: Message) {
        self.cache.entry(entity_id).or_default().push(message);
    }

    /// Retrieve from the cache all the messages related to an entity
    pub fn restore_messages(&mut self, entity_id: &EntityTopicId) -> Vec<Message> {
        self.birthed.insert(entity_id.to_owned());
        self.cache
            .remove(entity_id)
            .map(|q| q.into())
            .unwrap_or_default()
    }
}
