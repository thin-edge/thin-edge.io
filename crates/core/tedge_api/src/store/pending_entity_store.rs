use crate::entity_store::EntityRegistrationMessage;
use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::store::ring_buffer::RingBuffer;
use mqtt_channel::MqttMessage;
use std::collections::HashMap;
use tracing::error;

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

    /// Removes from the pending entity cache the registration messages
    /// of the direct children of a freshly registered entity.
    ///
    /// The data messages cached for these children,
    /// as well as the pending registrations of their own children, are left in the cache.
    /// They are taken only when a child is actually registered,
    /// using `take_cached_entity_data` and `take_cached_child_registrations`,
    /// so that they are not lost if the registration of that child is rejected.
    pub fn take_cached_child_registrations(
        &mut self,
        entity_tid: &EntityTopicId,
    ) -> Vec<EntityRegistrationMessage> {
        self.orphans
            .remove(entity_tid)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|child| {
                self.entities
                    .get_mut(&child)
                    .and_then(|cached_entity| cached_entity.reg_message.take())
            })
            .collect()
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
        let siblings = self.orphans.entry(parent).or_default();
        if !siblings.contains(&source) {
            siblings.push(source.clone());
        }
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

        let take_children = |store: &mut PendingEntityStore, parent: &str| {
            store
                .take_cached_child_registrations(
                    &EntityTopicId::default_child_device(parent).unwrap(),
                )
                .into_iter()
                .map(|e| e.topic_id)
                .collect::<Vec<EntityTopicId>>()
        };

        assert_eq!(
            take_children(&mut store, "child0"),
            vec![
                EntityTopicId::default_child_device("child00").unwrap(),
                EntityTopicId::default_child_device("child01").unwrap(),
            ]
        );
        assert_eq!(
            take_children(&mut store, "child00"),
            vec![EntityTopicId::default_child_device("child000").unwrap()]
        );
        assert_eq!(
            take_children(&mut store, "child000"),
            vec![EntityTopicId::default_child_device("child0000").unwrap()]
        );
        assert_eq!(
            take_children(&mut store, "child0000"),
            vec![EntityTopicId::default_child_device("child00000").unwrap()]
        );
        assert!(take_children(&mut store, "child0").is_empty());
    }

    #[test]
    fn taking_child_registrations_keeps_their_cached_data() {
        let mut store = build_pending_entity_store();
        let child = EntityTopicId::default_child_device("child0").unwrap();
        let grandchild = EntityTopicId::default_child_device("child00").unwrap();

        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(child.clone(), EntityType::ChildDevice)
                .with_parent(EntityTopicId::default_main_device()),
        );
        store.cache_early_registration_message(
            EntityRegistrationMessage::new_custom(grandchild.clone(), EntityType::ChildDevice)
                .with_parent(child.clone()),
        );
        let measurement = MqttMessage::new(
            &Topic::new_unchecked("te/device/child0///m/environment"),
            json!({"temperature": 50}).to_string(),
        );
        store.cache_early_data_message(measurement.clone());

        let children = store.take_cached_child_registrations(&EntityTopicId::default_main_device());
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].topic_id, child);

        // Taking the child registration leaves its data and its own children in the cache,
        // to be taken once the child is actually registered
        let child_data = store.take_cached_entity_data(children[0].clone());
        assert_eq!(child_data.data_messages, vec![measurement]);
        let grandchildren = store.take_cached_child_registrations(&child);
        assert_eq!(grandchildren.len(), 1);
        assert_eq!(grandchildren[0].topic_id, grandchild);
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
