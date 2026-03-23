use std::collections::HashMap;
use std::collections::HashSet;
use std::time::SystemTime;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::store::RingBuffer;
use tedge_flows::ConfigError;
use tedge_flows::FlowContextHandle;
use tedge_flows::FlowContextUpdate;
use tedge_flows::FlowError;
use tedge_flows::JsonValue;
use tedge_flows::Message;

/// Cache messages for an entity till the metadata of that entity are actually stored in the context.
///
/// - Cache messages from unregistered entity
/// - Publish downstream the messages cached for an entity
///   when the context is updated with the metadata for that entity
#[derive(Clone, Default)]
pub struct MessageCache {
    mqtt_schema: MqttSchema,
    cache: HashMap<String, RingBuffer<Message>>,
    birthed: HashSet<String>,
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
            Ok((entity_id, _)) if entity_id.is_default_main_device() => Ok(vec![message.clone()]),

            Ok((entity_id, _)) if !self.birthed.contains(entity_id.as_str()) => {
                self.cache_message(entity_id, message.clone());
                Ok(vec![])
            }

            _ => Ok(vec![message.clone()]),
        }
    }

    fn on_context_update(
        &mut self,
        _timestamp: SystemTime,
        update: &FlowContextUpdate,
        _context: &FlowContextHandle,
    ) -> Result<Vec<Message>, FlowError> {
        match update {
            FlowContextUpdate::Inserted { key } => Ok(self.restore_messages(key)),
            FlowContextUpdate::Removed { key } => {
                self.remove_messages(key);
                Ok(vec![])
            }
        }
    }
}

impl MessageCache {
    /// Cache a messages for an entity
    pub fn cache_message(&mut self, entity_id: EntityTopicId, message: Message) {
        self.cache
            .entry(entity_id.to_string())
            .or_default()
            .push(message);
    }

    /// Retrieve from the cache all the messages related to an entity
    pub fn restore_messages(&mut self, entity_id: &str) -> Vec<Message> {
        self.birthed.insert(entity_id.to_owned());
        self.cache
            .remove(entity_id)
            .map(|q| q.into())
            .unwrap_or_default()
    }

    /// remove from the cache all the messages related to an entity
    pub fn remove_messages(&mut self, entity_id: &str) {
        self.birthed.remove(entity_id);
        self.cache.remove(entity_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let result = on_context_update(&mut cache, &birth);
        assert_eq!(result, vec![measurement]);
    }

    #[test]
    fn messages_from_birthed_entity_pass_through() {
        let mut cache = make_cache();

        let birth = make_birth_message("device/child1//");
        let _ = on_context_update(&mut cache, &birth);

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
        assert_eq!(on_context_update(&mut cache, &birth1), vec![msg1]);

        let birth2 = make_birth_message("device/child2//");
        assert_eq!(on_context_update(&mut cache, &birth2), vec![msg2]);
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
        let result = on_context_update(&mut cache, &birth);
        assert_eq!(result, vec![msg1, msg2, msg3]);
    }

    fn make_cache() -> MessageCache {
        MessageCache::default()
    }

    fn make_message(topic: &str, payload: &str) -> Message {
        Message::new(topic, payload)
    }

    fn make_birth_message(entity_id: &str) -> FlowContextUpdate {
        FlowContextUpdate::Inserted {
            key: entity_id.to_string(),
        }
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

    fn on_context_update(cache: &mut MessageCache, update: &FlowContextUpdate) -> Vec<Message> {
        cache
            .on_context_update(
                SystemTime::UNIX_EPOCH,
                update,
                &FlowContextHandle::default(),
            )
            .unwrap()
    }
}
