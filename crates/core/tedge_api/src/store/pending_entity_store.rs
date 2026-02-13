use crate::entity_store::EntityRegistrationMessage;
use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::store::ring_buffer::RingBuffer;
use log::error;
use mqtt_channel::MqttMessage;
use std::collections::HashMap;

/// A store for all the entities for which data messages are received before
/// its registration message itself is received.
/// It also stores all the child device registration messages received before
/// their parents themselves are registered, including their data.
pub struct PendingEntityStore {
    mqtt_schema: MqttSchema,
    // This orphans map is keyed by the unregistered parent topic id to their children
    orphans: HashMap<EntityTopicId, Vec<EntityTopicId>>,
    entities: HashMap<EntityTopicId, PendingEntityCache>,
    telemetry_cache: RingBuffer<MqttMessage>,
}

/// A cache of all the data messages received before the entity itself is registered.
/// The telemetry messages are stored in a bounded buffer,
/// that replaces older values with newer values when full,
/// to make sure that only the most recent ones are cached to prevent unbounded growth.
/// Other metadata messages which are are stored in an unbounded vector,
/// as these are more critical data, none of which can be dropped.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PendingEntityCache {
    pub reg_message: Option<EntityRegistrationMessage>,
    pub metadata: Vec<MqttMessage>,
}

impl PendingEntityCache {
    fn new() -> Self {
        PendingEntityCache {
            reg_message: None,
            metadata: vec![],
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// Registration data for an entity that has been fully registered
///
/// Possibly with a list of MQTT messages
/// that have been delayed till this registration.
pub struct RegisteredEntityData {
    pub reg_message: EntityRegistrationMessage,
    pub data_messages: Vec<MqttMessage>,
}

impl From<EntityRegistrationMessage> for RegisteredEntityData {
    fn from(reg_message: EntityRegistrationMessage) -> Self {
        Self {
            reg_message,
            data_messages: vec![],
        }
    }
}

impl PendingEntityStore {
    pub fn new(mqtt_schema: MqttSchema, telemetry_cache_size: usize) -> Self {
        Self {
            mqtt_schema,
            orphans: HashMap::new(),
            entities: HashMap::new(),
            telemetry_cache: RingBuffer::new(telemetry_cache_size),
        }
    }

    pub fn take_cached_entity_data(
        &mut self,
        reg_message: EntityRegistrationMessage,
    ) -> RegisteredEntityData {
        let mut pending_messages = vec![];
        if let Some(pending_entity) = self.entities.remove(&reg_message.topic_id) {
            pending_messages.extend(pending_entity.metadata);
            pending_messages.extend(self.take_cached_telemetry_data(&reg_message.topic_id));
        }

        RegisteredEntityData {
            reg_message,
            data_messages: pending_messages,
        }
    }

    /// Recursively removes from the pending entity cache the children of a freshly registered device.
    ///
    /// Returns the list of devices which registration is no more pending.
    pub fn take_cached_child_entities_data(
        &mut self,
        entity_tid: &EntityTopicId,
    ) -> Vec<RegisteredEntityData> {
        let mut children = vec![];
        if let Some(direct_children) = self.orphans.remove(entity_tid) {
            for child in direct_children {
                if let Some(pending_entity_cache) = self.entities.remove(&child) {
                    let pending_entity_data = self.registered_data_from_cache(pending_entity_cache);
                    children.push(pending_entity_data);
                    children.append(&mut self.take_cached_child_entities_data(&child));
                }
            }
        }
        children
    }

    fn registered_data_from_cache(
        &mut self,
        pending_cache: PendingEntityCache,
    ) -> RegisteredEntityData {
        let reg_message = pending_cache.reg_message.unwrap();
        let mut pending_messages = vec![];
        pending_messages.extend(pending_cache.metadata);
        pending_messages.extend(self.take_cached_telemetry_data(&reg_message.topic_id));

        RegisteredEntityData {
            reg_message,
            data_messages: pending_messages,
        }
    }

    fn take_cached_telemetry_data(&mut self, entity_tid: &EntityTopicId) -> Vec<MqttMessage> {
        let mut messages = vec![];
        let telemetry_cache = self.telemetry_cache.take();
        for message in telemetry_cache.into_iter() {
            match self.mqtt_schema.entity_channel_of(&message.topic) {
                Ok((tid, _)) => {
                    if tid == entity_tid {
                        messages.push(message);
                    } else {
                        self.telemetry_cache.push(message)
                    }
                }
                Err(_) => self.telemetry_cache.push(message),
            }
        }
        messages
    }

    pub fn cache_early_data_message(&mut self, message: MqttMessage) {
        if let Ok((topic_id, channel)) = self.mqtt_schema.entity_channel_of(&message.topic) {
            let entity_cache = self
                .entities
                .entry(topic_id)
                .or_insert_with(PendingEntityCache::new);
            match &channel {
                Channel::Measurement { .. } | Channel::Event { .. } | Channel::Alarm { .. } => {
                    self.telemetry_cache.push(message);
                }
                Channel::EntityTwinData { .. }
                | Channel::MeasurementMetadata { .. }
                | Channel::EventMetadata { .. }
                | Channel::AlarmMetadata { .. }
                | Channel::Health
                | Channel::CommandMetadata { .. }
                | Channel::Command { .. } => entity_cache.metadata.push(message),
                _ => {
                    // Ignore
                }
            }
        } else {
            error!("Ignoring the message: {message:?} that does not conform to the expected  MQTT schema: {:?}", self.mqtt_schema);
        }
    }

    pub fn cache_early_registration_message(&mut self, reg_message: EntityRegistrationMessage) {
        let source = reg_message.topic_id.clone();
        let parent = reg_message.parent.clone().unwrap();
        self.orphans.entry(parent).or_default().push(source.clone());
        self.entities
            .entry(source)
            .and_modify(|cached_entity| {
                cached_entity.reg_message = Some(reg_message.clone());
            })
            .or_insert_with(|| {
                let mut cached_entity = PendingEntityCache::new();
                cached_entity.reg_message = Some(reg_message);
                cached_entity
            });
    }
}

#[cfg(test)]
mod tests {
    use mqtt_channel::MqttMessage;
    use mqtt_channel::Topic;
    use serde_json::json;

    use super::PendingEntityStore;
    use crate::entity::EntityType;
    use crate::entity_store::EntityRegistrationMessage;
    use crate::mqtt_topics::EntityTopicId;
    use crate::mqtt_topics::MqttSchema;

    #[test]
    fn take_cached_child_entities() {
        let mut store = build_pending_entity_store();

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_child_device("child00000").unwrap(),
                EntityType::ChildDevice,
            )
            .with_parent(EntityTopicId::default_child_device("child0000").unwrap()),
        );

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_child_device("child000").unwrap(),
                EntityType::ChildDevice,
            )
            .with_parent(EntityTopicId::default_child_device("child00").unwrap()),
        );

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_child_device("child00").unwrap(),
                EntityType::ChildDevice,
            )
            .with_parent(EntityTopicId::default_child_device("child0").unwrap()),
        );

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_child_device("child0000").unwrap(),
                EntityType::ChildDevice,
            )
            .with_parent(EntityTopicId::default_child_device("child000").unwrap()),
        );

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_child_device("child01").unwrap(),
                EntityType::ChildDevice,
            )
            .with_parent(EntityTopicId::default_child_device("child0").unwrap()),
        );

        let children = store.take_cached_child_entities_data(
            &EntityTopicId::default_child_device("child0").unwrap(),
        );

        assert_eq!(
            children
                .into_iter()
                .map(|e| e.reg_message.topic_id)
                .collect::<Vec<EntityTopicId>>(),
            vec![
                EntityTopicId::default_child_device("child00").unwrap(),
                EntityTopicId::default_child_device("child000").unwrap(),
                EntityTopicId::default_child_device("child0000").unwrap(),
                EntityTopicId::default_child_device("child00000").unwrap(),
                EntityTopicId::default_child_device("child01").unwrap(),
            ]
        )
    }

    #[test]
    fn take_cached_entity_filters_telemetry() {
        let mut store = build_pending_entity_store();

        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/environment"),
            json!({"temperature": 50}).to_string(),
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child2///m/environment"),
            json!({"temperature": 60}).to_string(),
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/environment"),
            json!({"pressure": 40}).to_string(),
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child3///m/environment"),
            json!({"pressure": 30}).to_string(),
        ));

        let cached_entity = store.take_cached_entity_data(EntityRegistrationMessage::new_custom(
            EntityTopicId::default_child_device("child1").unwrap(),
            EntityType::ChildDevice,
        ));
        assert_eq!(
            cached_entity.data_messages,
            vec![
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///m/environment"),
                    json!({"temperature": 50}).to_string(),
                ),
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///m/environment"),
                    json!({"pressure": 40}).to_string(),
                ),
            ]
        );

        let cached_entity = store.take_cached_entity_data(EntityRegistrationMessage::new_custom(
            EntityTopicId::default_child_device("child2").unwrap(),
            EntityType::ChildDevice,
        ));
        assert_eq!(
            cached_entity.data_messages,
            vec![MqttMessage::new(
                &Topic::new_unchecked("te/device/child2///m/environment"),
                json!({"temperature": 60}).to_string(),
            ),]
        );

        let cached_entity = store.take_cached_entity_data(EntityRegistrationMessage::new_custom(
            EntityTopicId::default_child_device("child3").unwrap(),
            EntityType::ChildDevice,
        ));
        assert_eq!(
            cached_entity.data_messages,
            vec![MqttMessage::new(
                &Topic::new_unchecked("te/device/child3///m/environment"),
                json!({"pressure": 30}).to_string(),
            ),]
        );
    }

    #[test]
    fn cached_entity_returns_metadata_before_telemetry() {
        let mut store = build_pending_entity_store();

        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/environment"),
            json!({"temperature": 50}).to_string(),
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///twin/maintenance_mode"),
            "true",
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///m/environment"),
            json!({"pressure": 40}).to_string(),
        ));
        store.cache_early_data_message(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///twin/service_count"),
            "5",
        ));

        let cached_entity = store.take_cached_entity_data(EntityRegistrationMessage::new_custom(
            EntityTopicId::default_child_device("child1").unwrap(),
            EntityType::ChildDevice,
        ));
        assert_eq!(
            cached_entity.data_messages,
            vec![
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///twin/maintenance_mode"),
                    "true",
                ),
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///twin/service_count"),
                    "5",
                ),
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///m/environment"),
                    json!({"temperature": 50}).to_string(),
                ),
                MqttMessage::new(
                    &Topic::new_unchecked("te/device/child1///m/environment"),
                    json!({"pressure": 40}).to_string(),
                ),
            ]
        );
    }

    fn build_pending_entity_store() -> PendingEntityStore {
        PendingEntityStore::new(MqttSchema::default(), 5)
    }
}
