use crate::mea::entities::C8yEntityBirth;
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
                Ok(vec![])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mea::entities::C8yEntityBirth;
    use crate::mea::entities::C8yEntityStatus;
    use tedge_flows::FlowContextHandle;
    use tedge_flows::Transformer;

    #[test]
    fn main_device_messages_are_never_cached() {
        let mut cache = make_cache();
        let msg = make_message("te/device/main///m/temperature", r#"{"temp":42.0}"#);
        let result = on_message(&mut cache, &msg);
        assert_eq!(result, vec![msg]);
    }

    #[test]
    fn messages_from_unregistered_entity_are_cached() {
        let mut cache = make_cache();
        let msg = make_message("te/device/child1///m/temperature", r#"{"temp":42.0}"#);
        let result = on_message(&mut cache, &msg);
        assert!(result.is_empty());
    }

    #[test]
    fn cached_messages_are_released_on_entity_birth() {
        let mut cache = make_cache();
        let measurement = make_message("te/device/child1///m/temperature", r#"{"temp":42.0}"#);
        assert!(on_message(&mut cache, &measurement).is_empty());

        let birth = make_birth_message("device/child1//");
        let result = on_message(&mut cache, &birth);
        assert_eq!(result, vec![measurement]);
    }

    #[test]
    fn messages_from_birthed_entity_pass_through() {
        let mut cache = make_cache();

        let birth = make_birth_message("device/child1//");
        let _ = on_message(&mut cache, &birth);

        let measurement = make_message("te/device/child1///m/temperature", r#"{"temp":99.0}"#);
        let result = on_message(&mut cache, &measurement);
        assert_eq!(result, vec![measurement]);
    }

    #[test]
    fn messages_on_unrecognized_topics_pass_through() {
        let mut cache = make_cache();
        let msg = make_message("some/unrelated/topic", r#"{"data":"value"}"#);
        let result = on_message(&mut cache, &msg);
        assert_eq!(result, vec![msg]);
    }

    #[test]
    fn messages_for_different_entities_are_cached_independently() {
        let mut cache = make_cache();
        let msg1 = make_message("te/device/child1///m/temperature", r#"{"temp":1.0}"#);
        let msg2 = make_message("te/device/child2///m/temperature", r#"{"temp":2.0}"#);
        assert!(on_message(&mut cache, &msg1).is_empty());
        assert!(on_message(&mut cache, &msg2).is_empty());

        let birth1 = make_birth_message("device/child1//");
        assert_eq!(on_message(&mut cache, &birth1), vec![msg1]);

        let birth2 = make_birth_message("device/child2//");
        assert_eq!(on_message(&mut cache, &birth2), vec![msg2]);
    }

    #[test]
    fn all_cached_messages_for_an_entity_are_released_on_birth() {
        let mut cache = make_cache();
        let msg1 = make_message("te/device/child1///m/temperature", r#"{"temp":1.0}"#);
        let msg2 = make_message("te/device/child1///m/temperature", r#"{"temp":2.0}"#);
        let msg3 = make_message("te/device/child1///e/click", r#"{"text":"clicked"}"#);
        assert!(on_message(&mut cache, &msg1).is_empty());
        assert!(on_message(&mut cache, &msg2).is_empty());
        assert!(on_message(&mut cache, &msg3).is_empty());

        let birth = make_birth_message("device/child1//");
        let result = on_message(&mut cache, &birth);
        assert_eq!(result, vec![msg1, msg2, msg3]);
    }

    fn make_cache() -> MessageCache {
        let mapper_id: EntityTopicId = "device/main/service/tedge-mapper-c8y".parse().unwrap();
        MessageCache::new(mapper_id)
    }

    fn make_message(topic: &str, payload: &str) -> Message {
        Message::new(topic, payload)
    }

    const BIRTH_TOPIC: &str = "te/device/main/service/tedge-mapper-c8y/status/entities";

    fn make_birth_message(entity_id: &str) -> Message {
        let birth = C8yEntityBirth {
            entity: entity_id.parse().unwrap(),
            status: C8yEntityStatus::Registered,
            time: 0.0,
        };
        make_message(BIRTH_TOPIC, &birth.to_json())
    }

    fn on_message(cache: &mut MessageCache, message: &Message) -> Vec<Message> {
        cache
            .on_message(
                SystemTime::UNIX_EPOCH,
                message,
                &FlowContextHandle::default(),
            )
            .unwrap()
    }
}
