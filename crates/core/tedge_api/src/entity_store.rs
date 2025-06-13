//! A store containing registered MQTT entities.
//!
//! References:
//!
//! - <https://github.com/thin-edge/thin-edge.io/issues/2081>
//! - <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#entity-store>

// TODO: move entity business logic to its own module

use crate::entity::EntityExternalId;
use crate::entity::EntityMetadata;
use crate::entity::EntityType;
use crate::entity::InsertOutcome;
use crate::entity_store;
use crate::mqtt_topics::default_topic_schema;
use crate::mqtt_topics::Channel;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::MqttSchema;
use crate::store::message_log::MessageLogReader;
use crate::store::message_log::MessageLogWriter;
use crate::store::pending_entity_store::PendingEntityStore;
use crate::store::pending_entity_store::RegisteredEntityData;
use log::debug;
use log::error;
use log::info;
use log::warn;
use mqtt_channel::MqttMessage;
use mqtt_channel::QoS;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value as JsonValue;
use std::collections::hash_map::Entry;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::mem;
use std::path::Path;

/// A store for topic-based entity metadata lookup.
///
/// This object is a hashmap from MQTT identifiers to entities (devices or
/// services) that publish on those topics. It keeps track of type of entities,
/// their relationships (parent and child entities), and other metadata.
///
/// The entity store takes as input registration messages published by entities
/// (devices and services) and stores information about entities and their
/// hierarchy, allowing to efficiently query it. It's possible to:
///
/// - enumerate all registered devices
/// - check if a given entity is already registered
/// - query services and child devices of a given device
/// - query parent of an entity
///
/// # Examples
///
/// ```
/// # use mqtt_channel::{MqttMessage, Topic};
/// # use tedge_api::mqtt_topics::MqttSchema;
/// # use tedge_api::entity_store::{EntityStore, EntityRegistrationMessage};
///
/// let registration_message = EntityRegistrationMessage::try_from(
///     "device/main//".parse().unwrap(),
///     r#"{"@type": "device"}"#.as_bytes()
/// ).unwrap();
///
/// let mut entity_store = EntityStore::with_main_device(
///     MqttSchema::default(),
///     registration_message,
///     0,
///     "/tmp",
///     true
/// );
/// ```
pub struct EntityStore {
    pub mqtt_schema: MqttSchema,
    main_device: EntityTopicId,
    entities: EntityTree,
    pending_entity_store: PendingEntityStore,
    // The persistent message log to persist entity registrations and twin data messages
    message_log: MessageLogWriter,
}

impl EntityStore {
    #[allow(clippy::too_many_arguments)]
    pub fn with_main_device<P>(
        mqtt_schema: MqttSchema,
        main_device: EntityRegistrationMessage,
        telemetry_cache_size: usize,
        log_dir: P,
        clean_start: bool,
    ) -> Result<Self, InitError>
    where
        P: AsRef<Path>,
    {
        if main_device.r#type != EntityType::MainDevice {
            return Err(InitError::Custom(
                "Provided main device is not of type main-device".into(),
            ));
        }

        let metadata = EntityMetadata {
            topic_id: main_device.topic_id.clone(),
            external_id: None,
            r#type: main_device.r#type,
            parent: None,
            health_endpoint: None,
            twin_data: main_device.twin_data,
            persistent_channels: BTreeSet::new(),
        };

        let message_log = if clean_start {
            MessageLogWriter::new_truncated(log_dir.as_ref()).map_err(|err| {
                InitError::Custom(format!(
                    "Loading the entity store log for writes failed with {err}",
                ))
            })?
        } else {
            MessageLogWriter::new(log_dir.as_ref()).map_err(|err| {
                InitError::Custom(format!(
                    "Loading the entity store log for writes failed with {err}",
                ))
            })?
        };

        let mut entity_store = EntityStore {
            mqtt_schema: mqtt_schema.clone(),
            main_device: main_device.topic_id.clone(),
            entities: EntityTree::new(main_device.topic_id, metadata),
            pending_entity_store: PendingEntityStore::new(mqtt_schema, telemetry_cache_size),
            message_log,
        };

        entity_store.load_from_message_log(log_dir.as_ref());

        Ok(entity_store)
    }

    pub fn load_from_message_log<P>(&mut self, log_dir: P)
    where
        P: AsRef<Path>,
    {
        info!("Loading the entity store from the log");
        match MessageLogReader::new(log_dir) {
            Err(err) => {
                error!(
                    "Failed to read the entity store log due to {err}. Ignoring and proceeding..."
                )
            }
            Ok(mut message_log_reader) => {
                loop {
                    match message_log_reader.next_message() {
                        Err(err) => {
                            error!("Parsing log entry failed with {err}");
                            continue;
                        }
                        Ok(None) => {
                            info!("Finished loading the entity store from the log");
                            return;
                        }
                        Ok(Some(message)) => {
                            if let Ok((source, channel)) =
                                self.mqtt_schema.entity_channel_of(&message.topic)
                            {
                                match channel {
                                    Channel::EntityMetadata => {
                                        if let Ok(register_message) =
                                            EntityRegistrationMessage::try_from(
                                                source.clone(),
                                                message.payload_bytes(),
                                            )
                                        {
                                            if let Err(err) = self.register_entity(register_message)
                                            {
                                                error!("Failed to re-register {source} from the persistent entity store due to {err}");
                                                continue;
                                            }
                                        }
                                    }
                                    Channel::EntityTwinData { fragment_key } => {
                                        let fragment_value = if message.payload_bytes().is_empty() {
                                            JsonValue::Null
                                        } else {
                                            match serde_json::from_slice::<JsonValue>(
                                                message.payload_bytes(),
                                            ) {
                                                Ok(json_value) => json_value,
                                                Err(err) => {
                                                    error!("Failed to parse twin fragment value of {fragment_key} of {source} from the persistent entity store due to {err}");
                                                    continue;
                                                }
                                            }
                                        };

                                        let twin_data = EntityTwinMessage::new(
                                            source.clone(),
                                            fragment_key,
                                            fragment_value,
                                        );
                                        if let Err(err) = self.set_twin_fragment(twin_data.clone())
                                        {
                                            error!("Failed to restore twin fragment: {twin_data:?} from the persistent entity store due to {err}");
                                            continue;
                                        }
                                    }
                                    Channel::CommandMetadata { .. } => {
                                        // Do nothing for now as supported operations are not part of the entity store
                                    }
                                    channel => {
                                        warn!(
                                            "Restoring messages on channel: {:?} not supported",
                                            channel
                                        )
                                    }
                                }
                            } else {
                                warn!(
                                    "Ignoring unsupported message retrieved from entity store: {:?}",
                                    message
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Iterates over the entity topic ids
    pub fn entity_topic_ids(&self) -> impl Iterator<Item = &EntityTopicId> {
        self.entities.entity_topic_ids()
    }

    /// Returns information about an entity under a given MQTT entity topic identifier.
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entities.get(entity_topic_id)
    }

    /// Returns a mutable reference to the `EntityMetadata` for the given `EntityTopicId`.
    fn get_mut(&mut self, entity_topic_id: &EntityTopicId) -> Option<&mut EntityMetadata> {
        self.entities.get_mut(entity_topic_id)
    }

    /// Tries to get information about an entity using its `EntityTopicId`,
    /// returning an error if the entity is not registered.
    pub fn try_get(&self, entity_topic_id: &EntityTopicId) -> Result<&EntityMetadata, Error> {
        self.get(entity_topic_id)
            .ok_or_else(|| Error::UnknownEntity(entity_topic_id.to_string()))
    }

    /// Tries to get a mutable reference to the `EntityMetadata` for the given `EntityTopicId`,
    /// returning an error if the entity is not registered.
    fn try_get_mut(
        &mut self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<&mut EntityMetadata, Error> {
        self.get_mut(entity_topic_id)
            .ok_or_else(|| Error::UnknownEntity(entity_topic_id.to_string()))
    }

    /// Returns the MQTT identifier of the main device.
    ///
    /// The main device is an entity with `@type: "device"`.
    pub fn main_device(&self) -> &EntityTopicId {
        &self.main_device
    }

    /// Returns MQTT identifiers of child devices of a given device.
    pub fn child_devices(&self, entity_topic: &EntityTopicId) -> Vec<&EntityTopicId> {
        self.entities
            .children(entity_topic)
            .into_iter()
            .filter(|(_, e)| e.r#type == EntityType::ChildDevice)
            .map(|(k, _)| k)
            .collect()
    }

    /// Returns MQTT identifiers of services running on a given device.
    pub fn services(&self, entity_topic: &EntityTopicId) -> Vec<&EntityTopicId> {
        self.entities
            .children(entity_topic)
            .into_iter()
            .filter(|(_, e)| e.r#type == EntityType::Service)
            .map(|(k, _)| k)
            .collect()
    }

    /// Updates entity store state based on the content of the entity registration message.
    ///
    /// Caches the entity if it cannot be registered because its ancestors are not registered yet.
    ///
    /// Returns a vector of registered entities that includes:
    /// - the entity that is provided in the input message (if actually new and not cached)
    /// - any previously cached child entities of the parent that is now registered.
    pub fn update(
        &mut self,
        message: EntityRegistrationMessage,
    ) -> Result<Vec<RegisteredEntityData>, Error> {
        match self.register_and_persist_entity(message.clone()) {
            Ok(affected_entities) => {
                if affected_entities.is_empty() {
                    Ok(vec![])
                } else {
                    let topic_id = message.topic_id.clone();
                    let current_entity_data =
                        self.pending_entity_store.take_cached_entity_data(message);
                    let mut pending_entities = vec![current_entity_data];

                    let pending_children = self
                        .pending_entity_store
                        .take_cached_child_entities_data(&topic_id);
                    for pending_child in pending_children {
                        let child_reg_message = pending_child.reg_message.clone();
                        self.register_and_persist_entity(child_reg_message.clone())?;
                        pending_entities.push(pending_child);
                    }

                    Ok(pending_entities)
                }
            }
            Err(Error::NoParent(_)) => {
                // When a child device registration message is received before the parent is registered,
                // cache it in the unregistered entity store to be processed later
                self.pending_entity_store
                    .cache_early_registration_message(message);
                Ok(vec![])
            }
            Err(err) => Err(err),
        }
    }

    fn register_entity(
        &mut self,
        message: EntityRegistrationMessage,
    ) -> Result<Vec<EntityTopicId>, Error> {
        debug!("Processing entity registration message, {:?}", message);
        let topic_id = message.topic_id.clone();

        let mut affected_entities = vec![];

        let parent = match message.r#type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => message
                .parent
                .clone()
                .or_else(|| Some(self.main_device.clone())),
            EntityType::Service => message
                .parent
                .clone()
                .or_else(|| topic_id.default_service_parent_identifier())
                .or_else(|| Some(self.main_device.clone())),
        };

        // parent device is affected if new device is its child
        if let Some(parent) = &parent {
            if !self.entities.contains_key(parent) {
                return Err(Error::NoParent(parent.to_string().into_boxed_str()));
            }

            affected_entities.push(parent.clone());
        }

        let entity_metadata = EntityMetadata {
            topic_id: topic_id.clone(),
            r#type: message.r#type,
            external_id: message.external_id,
            parent,
            health_endpoint: message.health_endpoint,
            twin_data: message.twin_data,
            persistent_channels: BTreeSet::new(),
        };

        match self.entities.insert(topic_id.clone(), entity_metadata) {
            InsertOutcome::Unchanged => Ok(vec![]),
            InsertOutcome::Inserted => Ok(affected_entities),
            InsertOutcome::Updated => {
                affected_entities.push(topic_id);
                Ok(affected_entities)
            }
        }
    }

    fn register_and_persist_entity(
        &mut self,
        message: EntityRegistrationMessage,
    ) -> Result<Vec<EntityTopicId>, Error> {
        let affected_entities = self.register_entity(message.clone())?;
        if !affected_entities.is_empty() {
            self.message_log
                .append_message(&message.to_mqtt_message(&self.mqtt_schema))?;
        }

        Ok(affected_entities)
    }

    pub fn update_entity(
        &mut self,
        topic_id: &EntityTopicId,
        update_message: EntityUpdateMessage,
    ) -> Result<&EntityMetadata, Error> {
        self.entities.update_entity(topic_id, update_message)
    }

    pub fn ancestors(&self, topic_id: &EntityTopicId) -> Result<Vec<&EntityTopicId>, Error> {
        self.entities.ancestors(topic_id)
    }

    /// Performs auto-registration process for an entity under a given
    /// identifier.
    ///
    /// If an entity is a service, its parent device is also auto-registered if it's
    /// not already registered.
    ///
    /// It returns MQTT register messages for the given entities to be published
    /// by the mapper, so other components can also be aware of a new device
    /// being registered.
    pub fn auto_register_entity(
        &mut self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<Vec<EntityRegistrationMessage>, entity_store::Error> {
        let auto_entities = default_topic_schema::parse(entity_topic_id);
        if auto_entities.is_empty() {
            return Err(Error::NonDefaultTopicScheme(entity_topic_id.clone()));
        };

        let mut register_messages = vec![];
        for auto_entity in auto_entities {
            // Skip any already registered entity
            if auto_entity.r#type != EntityType::MainDevice
                && self.get(&auto_entity.topic_id).is_none()
            {
                register_messages.push(auto_entity.clone());
                self.update(auto_entity)?;
            }
        }

        Ok(register_messages)
    }

    /// Recursively deregister an entity, its child devices and services
    pub fn deregister_entity(&mut self, topic_id: &EntityTopicId) -> Vec<EntityMetadata> {
        let mut removed_entities = vec![];
        self.entities.remove(topic_id, &mut removed_entities);
        removed_entities
    }

    /// Recursively deregister an entity, its child devices and services
    ///
    /// Persist the deregistration message
    pub fn deregister_and_persist_entity(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Vec<EntityMetadata>, Error> {
        let removed_entities = self.deregister_entity(topic_id);

        if !removed_entities.is_empty() {
            let topic = self
                .mqtt_schema
                .topic_for(topic_id, &Channel::EntityMetadata);
            let message = MqttMessage::new(&topic, "")
                .with_retain()
                .with_qos(QoS::AtLeastOnce);
            self.message_log.append_message(&message)?;
        }

        Ok(removed_entities)
    }

    pub fn get_twin_fragment(
        &self,
        topic_id: &EntityTopicId,
        fragment_key: &str,
    ) -> Option<&JsonValue> {
        self.entities
            .get(topic_id)
            .and_then(|entity| entity.twin_data.get(fragment_key))
    }

    /// Updates the entity twin data with the provided fragment data.
    /// Returns `true`, if the twin data got updated with the new fragment value.
    /// If the provided fragment already existed, `false` is returned.
    pub fn update_twin_fragment(
        &mut self,
        twin_message: EntityTwinMessage,
    ) -> Result<bool, entity_store::Error> {
        let updated = self.set_twin_fragment(twin_message.clone())?;
        if updated {
            self.message_log
                .append_message(&twin_message.to_mqtt_message(&self.mqtt_schema))?;
        }

        Ok(updated)
    }

    pub fn set_twin_fragment(
        &mut self,
        twin_message: EntityTwinMessage,
    ) -> Result<bool, entity_store::Error> {
        let fragment_key = twin_message.fragment_key;
        let fragment_value = twin_message.fragment_value;

        Self::validate_fragment_key(&fragment_key)?;

        let channel = Channel::EntityTwinData {
            fragment_key: fragment_key.clone(),
        };
        if !fragment_value.is_null() {
            // If the value is not null, we are tracking the fragment
            self.track_persistent_channel(&twin_message.topic_id, channel);
        } else {
            // If the value is null, we are removing the fragment
            self.untrack_persistent_channel(&twin_message.topic_id, &channel);
        }

        let entity = self.try_get_mut(&twin_message.topic_id)?;
        if fragment_value.is_null() {
            let existing = entity.twin_data.remove(&fragment_key);
            if existing.is_none() {
                return Ok(false);
            }
        } else {
            let existing = entity
                .twin_data
                .insert(fragment_key, fragment_value.clone());
            if existing.is_some_and(|v| v.eq(&fragment_value)) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn validate_fragment_key(fragment_key: &str) -> Result<(), entity_store::Error> {
        if fragment_key.is_empty() || fragment_key.starts_with('@') || fragment_key.contains('/') {
            return Err(Error::InvalidTwinData(fragment_key.to_string()));
        }
        Ok(())
    }

    pub fn get_twin_fragments(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<&Map<String, JsonValue>, entity_store::Error> {
        let entity = self.try_get(topic_id)?;
        Ok(&entity.twin_data)
    }

    pub fn set_twin_fragments(
        &mut self,
        topic_id: &EntityTopicId,
        fragments: Map<String, JsonValue>,
    ) -> Result<Map<String, JsonValue>, entity_store::Error> {
        for key in fragments.keys() {
            Self::validate_fragment_key(key)?;
        }
        let entity = self.try_get_mut(topic_id)?;
        let old = mem::replace(&mut entity.twin_data, Map::new());
        for (key, _) in old.iter() {
            self.untrack_persistent_channel(
                topic_id,
                &Channel::EntityTwinData {
                    fragment_key: key.clone(),
                },
            );
        }

        for (fragment_key, fragment_value) in fragments {
            self.update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                fragment_key,
                fragment_value,
            ))?;
        }
        Ok(old)
    }

    pub fn cache_early_data_message(&mut self, message: MqttMessage) {
        self.pending_entity_store.cache_early_data_message(message)
    }

    pub fn list_entity_tree(&self, filters: ListFilters) -> Vec<&EntityMetadata> {
        self.entities.list_entity_tree(filters)
    }

    pub fn track_persistent_channel(&mut self, topic_id: &EntityTopicId, channel: Channel) {
        self.entities.track_persistent_channel(topic_id, channel)
    }

    pub fn untrack_persistent_channel(&mut self, topic_id: &EntityTopicId, channel: &Channel) {
        self.entities.untrack_persistent_channel(topic_id, channel)
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListFilters {
    pub root: Option<EntityTopicId>,
    pub parent: Option<EntityTopicId>,
    pub r#type: Option<EntityType>,
}

impl ListFilters {
    pub fn root(mut self, value: EntityTopicId) -> Self {
        self.root = Some(value);
        self
    }

    pub fn parent(mut self, value: EntityTopicId) -> Self {
        self.parent = Some(value);
        self
    }

    pub fn r#type(mut self, value: EntityType) -> Self {
        self.r#type = Some(value);
        self
    }

    fn matches(&self, metadata: &EntityMetadata) -> bool {
        if let Some(entity_type) = self.r#type.as_ref() {
            if &metadata.r#type != entity_type {
                return false;
            }
        }
        if let Some(parent) = self.parent.as_ref() {
            if metadata.parent.as_ref() != Some(parent) {
                return false;
            }
        }
        true
    }
}

/// In-memory representation of the entity tree
struct EntityTree {
    main_device: EntityTopicId,
    entities: HashMap<EntityTopicId, EntityNode>,
}

#[derive(Debug)]
struct EntityNode {
    metadata: EntityMetadata,
    children: BTreeSet<EntityTopicId>,
}

impl EntityNode {
    pub fn new(metadata: EntityMetadata) -> Self {
        EntityNode {
            metadata,
            children: BTreeSet::new(),
        }
    }

    pub fn metadata(&self) -> &EntityMetadata {
        &self.metadata
    }
    pub fn mut_metadata(&mut self) -> &mut EntityMetadata {
        &mut self.metadata
    }
}

impl EntityTree {
    /// Create the tree of entities for the main device, given its name, topic id and metadata
    pub fn new(topic_id: EntityTopicId, metadata: EntityMetadata) -> Self {
        EntityTree {
            main_device: topic_id.clone(),
            entities: HashMap::from([(topic_id, EntityNode::new(metadata))]),
        }
    }

    pub fn contains_key(&self, topic_id: &EntityTopicId) -> bool {
        self.entities.contains_key(topic_id)
    }

    /// Iterate over the entity topic ids
    pub fn entity_topic_ids(&self) -> impl Iterator<Item = &EntityTopicId> {
        self.entities.keys()
    }

    /// Returns information about an entity under a given MQTT entity topic identifier.
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entities.get(entity_topic_id).map(EntityNode::metadata)
    }

    /// Returns a mutable reference to the `EntityMetadata` for the given `EntityTopicId`.
    fn get_mut(&mut self, entity_topic_id: &EntityTopicId) -> Option<&mut EntityMetadata> {
        self.entities
            .get_mut(entity_topic_id)
            .map(EntityNode::mut_metadata)
    }

    /// Tries to get information about an entity using its `EntityTopicId`,
    /// returning an error if the entity is not registered.
    fn try_get(&self, topic_id: &EntityTopicId) -> Result<&EntityMetadata, Error> {
        self.get(topic_id)
            .ok_or_else(|| Error::UnknownEntity(topic_id.to_string()))
    }

    fn try_get_entity_node_mut(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<&mut EntityNode, Error> {
        self.entities
            .get_mut(topic_id)
            .ok_or_else(|| Error::UnknownEntity(topic_id.to_string()))
    }

    /// All the entities with a given parent.
    pub fn children(&self, parent_id: &EntityTopicId) -> Vec<(&EntityTopicId, &EntityMetadata)> {
        let Some(children) = self.entities.get(parent_id).map(|node| &node.children) else {
            return vec![];
        };

        children
            .iter()
            .filter_map(|topic_id| self.entities.get_key_value(topic_id))
            .map(|(k, v)| (k, v.metadata()))
            .collect()
    }

    /// Insert a new entity
    ///
    /// Return Inserted if the entity is new
    /// Return Updated if the entity was previously registered and has been updated by this call
    /// Return Unchanged if the entity not affected by this call
    pub fn insert(
        &mut self,
        topic_id: EntityTopicId,
        entity_metadata: EntityMetadata,
    ) -> InsertOutcome {
        let maybe_parent = entity_metadata.parent.clone();
        let previous = self.entities.entry(topic_id.clone());
        let outcome = match previous {
            Entry::Occupied(mut occupied) => {
                // if there is no change, no entities were affected
                let existing_entity = occupied.get().metadata.clone();
                let existing_children = occupied.get().children.clone();

                let mut merged_other = existing_entity.twin_data.clone();
                merged_other.extend(entity_metadata.twin_data.clone());
                let merged_entity = EntityMetadata {
                    twin_data: merged_other,
                    persistent_channels: existing_entity.persistent_channels.clone(),
                    ..entity_metadata
                };

                if existing_entity == merged_entity {
                    InsertOutcome::Unchanged
                } else {
                    let updated_entity = EntityNode {
                        metadata: merged_entity,
                        children: existing_children,
                    };
                    occupied.insert(updated_entity);
                    InsertOutcome::Updated
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(EntityNode::new(entity_metadata));
                InsertOutcome::Inserted
            }
        };

        if let Some(parent) = maybe_parent {
            if let Some(parent_entry) = self.entities.get_mut(&parent) {
                parent_entry.children.insert(topic_id);
            }
        }

        debug!("Updated entity map: {:?}", self.entities);
        outcome
    }

    /// Recursively remove an entity, its child devices and services
    ///
    /// Populate the given vector with the metadata of the removed entities
    fn remove(&mut self, topic_id: &EntityTopicId, removed_entities: &mut Vec<EntityMetadata>) {
        if let Some(node) = self.entities.remove(topic_id) {
            removed_entities.push(node.metadata);
            let children = node.children;
            children
                .iter()
                .for_each(|sub_topic| self.remove(sub_topic, removed_entities));
        }
    }

    pub fn list_entity_tree(&self, filters: ListFilters) -> Vec<&EntityMetadata> {
        let start_root = filters
            .root
            .as_ref()
            .or(filters.parent.as_ref())
            .unwrap_or(&self.main_device);
        if self.entities.contains_key(start_root) {
            let mut topic_ids = VecDeque::from(vec![start_root]);
            let mut entities = vec![];

            while let Some(topic_id) = topic_ids.pop_front() {
                let metadata = self
                    .entities
                    .get(topic_id)
                    .map(|node| node.metadata())
                    .unwrap();
                if filters.matches(metadata) {
                    entities.push(metadata);
                }

                let (child_topics, _): (Vec<_>, Vec<_>) =
                    self.children(topic_id).into_iter().unzip();

                // If the `parent` filter is used, no need to search beyond the direct children of that parent
                if filters
                    .parent
                    .as_ref()
                    .map_or(true, |parent| parent == topic_id)
                {
                    topic_ids.extend(child_topics);
                }
            }
            entities
        } else {
            vec![]
        }
    }

    fn get_parent(&self, topic_id: &EntityTopicId) -> Result<&EntityTopicId, Error> {
        let parent = self
            .try_get(topic_id)?
            .parent
            .as_ref()
            .unwrap_or(&self.main_device);
        Ok(parent)
    }

    pub fn update_entity(
        &mut self,
        topic_id: &EntityTopicId,
        update_message: EntityUpdateMessage,
    ) -> Result<&EntityMetadata, Error> {
        if let Some(new_parent) = update_message.parent {
            if new_parent == topic_id {
                return Err(Error::InvalidSelfParent(new_parent.clone()));
            }

            if topic_id == &self.main_device {
                // The main device can not have a parent
                return Err(Error::InvalidMainDeviceParent);
            }

            let entity = self
                .try_get(&new_parent)
                .map_err(|_| Error::NoParent(new_parent.to_string().into_boxed_str()))?;
            if entity.r#type == EntityType::Service {
                return Err(Error::InvalidServiceParent(
                    new_parent.clone(),
                    topic_id.clone(),
                ));
            }

            if self.ancestors(&new_parent)?.contains(&topic_id) {
                return Err(Error::InvalidDescendentParent(
                    new_parent.clone(),
                    topic_id.clone(),
                ));
            }

            let current_parent = self.get_parent(topic_id)?.clone();
            if current_parent != new_parent {
                let current_node = self.try_get_entity_node_mut(topic_id)?;
                current_node.metadata.parent = Some(new_parent.clone());

                let new_parent_node = self.try_get_entity_node_mut(&new_parent)?;
                new_parent_node.children.insert(topic_id.clone());

                self.entities
                    .get_mut(&current_parent)
                    .expect("Parent entity should exist")
                    .children
                    .remove(topic_id);
            }
        }

        if let Some(health_endpoint) = update_message.health_endpoint {
            let health_endpoint_entity = self
                .try_get(&health_endpoint)
                .map_err(|_| Error::UnknownHealthEndpoint(health_endpoint.clone()))?;
            if health_endpoint_entity.r#type != EntityType::Service {
                return Err(Error::InvalidHealthEndpoint(
                    health_endpoint.clone(),
                    topic_id.clone(),
                ));
            }
            let entity = self.try_get_entity_node_mut(topic_id)?;
            entity.metadata.health_endpoint = Some(health_endpoint);
        }

        self.try_get(topic_id)
    }

    pub fn ancestors(&self, topic_id: &EntityTopicId) -> Result<Vec<&EntityTopicId>, Error> {
        let mut ancestors = vec![];
        let mut current = topic_id;
        while let Some(parent) = self.try_get(current)?.parent.as_ref() {
            ancestors.push(parent);
            current = parent;
        }
        Ok(ancestors)
    }

    pub fn track_persistent_channel(&mut self, topic_id: &EntityTopicId, channel: Channel) {
        self.get_mut(topic_id)
            .map(|entity| entity.persistent_channels.insert(channel));
    }

    pub fn untrack_persistent_channel(&mut self, topic_id: &EntityTopicId, channel: &Channel) {
        self.get_mut(topic_id)
            .map(|entity| entity.persistent_channels.remove(channel));
    }
}

/// Represents an error encountered while updating the store.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("The specified parent {0:?} does not exist in the entity store")]
    NoParent(Box<str>),

    #[error("Main device was not registered. Before registering child entities, register the main device")]
    NoMainDevice,

    #[error("The main device was already registered at topic {0}")]
    MainDeviceAlreadyRegistered(Box<str>),

    #[error("An entity with topic id: {0} is already registered")]
    EntityAlreadyRegistered(EntityTopicId),

    #[error("The specified entity: {0} does not exist in the entity store")]
    UnknownEntity(String),

    #[error("Auto registration of the entity with topic id {0} failed as it does not match the default topic scheme: 'device/<device-id>/service/<service-id>'. Try explicit registration instead.")]
    NonDefaultTopicScheme(EntityTopicId),

    // In practice won't be thrown because usually it is a map
    // TODO: remove this error variant when `EntityRegistrationMessage` is changed
    #[error("`EntityRegistrationMessage::other` field needs to be a Map")]
    EntityRegistrationOtherNotMap,

    #[error(transparent)]
    FromStdIoError(#[from] std::io::Error),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error("Invalid twin key: '{0}'. Keys that are empty, containing '/' or starting with '@' are not allowed")]
    InvalidTwinData(String),

    #[error("Entity: '{0}' can not be its own parent")]
    InvalidSelfParent(EntityTopicId),

    #[error("Entity: '{0}' can not be a parent of '{1}' because it is a service. Only devices can be parents")]
    InvalidServiceParent(EntityTopicId, EntityTopicId),

    #[error("Entity: '{0}' can not be a parent of '{1}' because '{0}' is a descendent of '{1}'")]
    InvalidDescendentParent(EntityTopicId, EntityTopicId),

    #[error("The parent of main device can not be updated")]
    InvalidMainDeviceParent,

    #[error("Updating the entity type of {0} is not supported")]
    InvalidEntityTypeUpdate(EntityTopicId),

    #[error("Entity: '{0}' can not be a health endpoint of '{1}' because it is not a service. Only services can be health endpoints")]
    InvalidHealthEndpoint(EntityTopicId, EntityTopicId),

    #[error("The specified health endpoint: {0} does not exist in the entity store")]
    UnknownHealthEndpoint(EntityTopicId),
}

#[derive(thiserror::Error, Debug)]
pub enum InitError {
    #[error(transparent)]
    FromError(#[from] Error),

    #[error(transparent)]
    FromStdIoError(#[from] std::io::Error),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error("Initialization failed with: {0}")]
    Custom(String),
}

/// An object representing the common entity registration message payload,
/// excluding the topic-id which is derived from different sources for MQTT and HTTP:
/// - the topic of the MQTT message
/// - the payload of the HTTP message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRegistrationPayload {
    #[serde(rename = "@id", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<EntityExternalId>,
    #[serde(rename = "@type")]
    pub r#type: EntityType,
    #[serde(rename = "@parent", skip_serializing_if = "Option::is_none")]
    pub parent: Option<EntityTopicId>,
    #[serde(rename = "@health", skip_serializing_if = "Option::is_none")]
    pub health_endpoint: Option<EntityTopicId>,

    #[serde(flatten)]
    pub twin_data: Map<String, JsonValue>,
}

/// An object representing a valid entity registration message,
/// including the topic-id which is derived from:
/// - the topic of the MQTT message
/// - the payload of the HTTP message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRegistrationMessage {
    // fields used by thin-edge locally
    pub topic_id: EntityTopicId,
    pub external_id: Option<EntityExternalId>,
    pub r#type: EntityType,
    pub parent: Option<EntityTopicId>,
    pub health_endpoint: Option<EntityTopicId>,

    pub twin_data: Map<String, JsonValue>,
}

impl EntityRegistrationMessage {
    pub fn try_from(topic_id: EntityTopicId, payload: &[u8]) -> Result<Self, serde_json::Error> {
        let payload: EntityRegistrationPayload = serde_json::from_slice(payload)?;

        Ok(Self {
            topic_id,
            external_id: payload.external_id,
            r#type: payload.r#type,
            parent: payload.parent,
            health_endpoint: payload.health_endpoint,
            twin_data: payload.twin_data,
        })
    }

    pub fn new_custom(topic_id: EntityTopicId, r#type: EntityType) -> Self {
        EntityRegistrationMessage {
            topic_id,
            r#type,
            external_id: None,
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
        }
    }

    pub fn with_parent(mut self, parent: EntityTopicId) -> Self {
        let _ = self.parent.insert(parent);
        self
    }

    pub fn with_external_id(mut self, external_id: EntityExternalId) -> Self {
        let _ = self.external_id.insert(external_id);
        self
    }

    pub fn with_twin_fragment(mut self, key: String, value: JsonValue) -> Self {
        let _ = self.twin_data.insert(key, value);
        self
    }

    pub fn with_health_endpoint(mut self, health_endpoint: EntityTopicId) -> Self {
        let _ = self.health_endpoint.insert(health_endpoint);
        self
    }

    /// Creates a entity registration message for a main device.
    pub fn main_device(main_device_id: Option<String>) -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: main_device_id.map(|v| v.into()),
            r#type: EntityType::MainDevice,
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
        }
    }

    // TODO: manual serialize impl
    pub fn to_mqtt_message(mut self, mqtt_schema: &MqttSchema) -> MqttMessage {
        let mut props = serde_json::Map::new();

        props.insert("@type".to_string(), self.r#type.to_string().into());

        if let Some(external_id) = self.external_id {
            props.insert("@id".to_string(), external_id.as_ref().to_string().into());
        }

        if let Some(parent) = self.parent {
            props.insert("@parent".to_string(), parent.to_string().into());
        }

        if let Some(health_endpoint) = self.health_endpoint {
            props.insert("@health".to_string(), health_endpoint.to_string().into());
        }

        props.append(&mut self.twin_data);

        let message = serde_json::to_string(&props).unwrap();

        let message_topic = mqtt_schema.topic_for(&self.topic_id, &Channel::EntityMetadata);
        MqttMessage::new(&message_topic, message).with_retain()
    }
}

impl From<&EntityMetadata> for EntityRegistrationMessage {
    fn from(value: &EntityMetadata) -> Self {
        EntityRegistrationMessage {
            topic_id: value.topic_id.clone(),
            r#type: value.r#type.clone(),
            external_id: value.external_id.clone(),
            parent: value.parent.clone(),
            health_endpoint: value.health_endpoint.clone(),
            twin_data: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityUpdateMessage {
    #[serde(rename = "@parent")]
    pub parent: Option<EntityTopicId>,

    #[serde(rename = "@health")]
    pub health_endpoint: Option<EntityTopicId>,
}

impl EntityUpdateMessage {
    pub fn with_parent(mut self, parent: EntityTopicId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_health_endpoint(mut self, health_endpoint: EntityTopicId) -> Self {
        self.health_endpoint = Some(health_endpoint);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTwinMessage {
    pub topic_id: EntityTopicId,
    pub fragment_key: String,
    pub fragment_value: JsonValue,
}

impl EntityTwinMessage {
    pub fn new(topic_id: EntityTopicId, fragment_key: String, fragment_value: JsonValue) -> Self {
        EntityTwinMessage {
            topic_id,
            fragment_key,
            fragment_value,
        }
    }

    pub fn to_mqtt_message(self, mqtt_schema: &MqttSchema) -> MqttMessage {
        let message_topic = mqtt_schema.topic_for(
            &self.topic_id,
            &Channel::EntityTwinData {
                fragment_key: self.fragment_key,
            },
        );
        let payload = if self.fragment_value.is_null() {
            "".to_string()
        } else {
            self.fragment_value.to_string()
        };
        MqttMessage::new(&message_topic, payload).with_retain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::str::FromStr;
    use tempfile::TempDir;
    use test_case::test_case;

    #[test]
    fn parse_entity_registration_message() {
        let parsed = EntityRegistrationMessage::try_from(
            "device/child1//".parse().unwrap(),
            json!({
                "@type" : "child-device",
                "name": "child1",
                "type": "RPi",
                "version": "5",
                "complex": {
                    "foo" : "bar"
                }
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        assert_eq!(parsed.r#type, EntityType::ChildDevice);
        assert_eq!(parsed.twin_data.get("name").unwrap(), "child1");
        assert_eq!(parsed.twin_data.get("type").unwrap(), "RPi");
        assert_eq!(parsed.twin_data.get("version").unwrap(), "5");
        assert_eq!(
            parsed.twin_data.get("complex").unwrap().get("foo").unwrap(),
            "bar"
        );
    }

    #[test_case(
        json!({
            "@type" : "main-device",
        }),
        "unknown variant";
        "invalid_entity_type"
    )]
    #[test_case(
        json!({
            "@id" : "child01",
        }),
        "missing field `@type`";
        "missing_entity_type"
    )]
    #[test_case(
        json!({
            "@type" : "child-device",
            "@id": 55
        }),
        "invalid type: integer `55`, expected a string";
        "invalid_external_id"
    )]
    #[test_case(
        json!({
            "@type" : "child-device",
            "@parent": "a/b/c/d/e"
        }),
        "An entity topic identifier has at most 4 segments";
        "invalid_parent"
    )]
    #[test_case(
        json!({
            "@type" : "child-device",
            "@health": "a/b/c/d/e"
        }),
        "An entity topic identifier has at most 4 segments";
        "invalid_health_endpoint"
    )]
    fn invalid_entity_registration_message(payload: JsonValue, expected: &str) {
        let error = EntityRegistrationMessage::try_from(
            "device/child1//".parse().unwrap(),
            payload.to_string().as_bytes(),
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains(expected),
            "Actual: \"{error}\" does not contain expected: \"{expected}\""
        );
    }

    #[test]
    fn registers_main_device() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store = new_entity_store(&temp_dir, true);

        assert_eq!(store.main_device(), &EntityTopicId::default_main_device());
        assert!(store.get(&EntityTopicId::default_main_device()).is_some());
    }

    #[test]
    fn register_child_device() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let entity = EntityRegistrationMessage::try_from(
            "device/child1//".parse().unwrap(),
            json!({
                "@type" : "child-device",
                "name": "child1",
                "type": "RPi",
                "version": "5",
                "complex": {
                    "foo" : "bar"
                }
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let updated_entities = store.update(entity.clone()).unwrap();

        let pending_entity: RegisteredEntityData = entity.into();
        assert_eq!(updated_entities.len(), 1);
        assert_eq!(updated_entities, vec![pending_entity]);
    }

    #[test]
    fn lists_child_devices() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        // If the @parent info is not provided, it is assumed to be an immediate
        // child of the main device.
        let updated_entities = store
            .update(
                EntityRegistrationMessage::try_from(
                    "device/child1//".parse().unwrap(),
                    json!({"@type": "child-device"}).to_string().as_bytes(),
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            updated_entities.get(0).unwrap().reg_message.topic_id,
            "device/child1//"
        );
        assert_eq!(
            store.child_devices(&EntityTopicId::default_main_device()),
            ["device/child1//"]
        );

        let updated_entities = store
            .update(
                EntityRegistrationMessage::try_from(
                    "device/child2//".parse().unwrap(),
                    json!({"@type": "child-device", "@parent": "device/main//"})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            updated_entities.get(0).unwrap().reg_message.topic_id,
            "device/child2//"
        );
        let children = store.child_devices(&EntityTopicId::default_main_device());
        assert!(children.iter().any(|&e| e == "device/child1//"));
        assert!(children.iter().any(|&e| e == "device/child2//"));
    }

    #[test_case(
        ListFilters::default(),
        BTreeSet::from([
            "device/main//",
            "device/main/service/service0",
            "device/main/service/service1",
            "device/child0//",
            "device/child00//",
            "device/child000//",
            "device/child1//",
            "device/child1/service/service10",
            "device/child2//",
            "device/child2/service/service20",
            "device/child2/service/service21",
            "device/child20//",
            "device/child21//",
            "device/child21/service/service210",
            "device/child210//",
            "device/child211//",
            "device/child2100//",
            "device/child22//",
        ]);
        "all_entities"
    )]
    #[test_case(
        ListFilters::default()
            .root("device/child2//".parse().unwrap()), 
        BTreeSet::from([
            "device/child2//",
            "device/child2/service/service20",
            "device/child2/service/service21",
            "device/child20//",
            "device/child21//",
            "device/child21/service/service210",
            "device/child210//",
            "device/child211//",
            "device/child2100//",
            "device/child22//",
        ]);
        "child_root"
    )]
    #[test_case(
        ListFilters::default()
            .parent("device/child2//".parse().unwrap()), 
        BTreeSet::from([
            "device/child2/service/service20",
            "device/child2/service/service21",
            "device/child20//",
            "device/child21//",
            "device/child22//",
        ]);
        "children_of_parent"
    )]
    #[test_case(
        ListFilters::default()
            .r#type(EntityType::ChildDevice), 
        BTreeSet::from([
            "device/child0//",
            "device/child1//",
            "device/child2//",
            "device/child00//",
            "device/child20//",
            "device/child21//",
            "device/child22//",
            "device/child000//",
            "device/child210//",
            "device/child211//",
            "device/child2100//",
        ]);
        "child_devices_only"
    )]
    #[test_case(
        ListFilters::default()
            .r#type(EntityType::Service), 
        BTreeSet::from([
            "device/main/service/service0",
            "device/main/service/service1",
            "device/child1/service/service10",
            "device/child2/service/service20",
            "device/child2/service/service21",
            "device/child21/service/service210",
        ]);
        "services_only"
    )]
    #[test_case(
        ListFilters::default()
            .root("device/child2//".parse().unwrap())
            .r#type(EntityType::ChildDevice), 
        BTreeSet::from([
            "device/child2//",
            "device/child20//",
            "device/child21//",
            "device/child22//",
            "device/child210//",
            "device/child211//",
            "device/child2100//",
        ]);
        "child_device_tree_of_child_root"
    )]
    #[test_case(
        ListFilters::default()
            .parent("device/child2//".parse().unwrap())
            .r#type(EntityType::ChildDevice), 
        BTreeSet::from([
            "device/child20//",
            "device/child21//",
            "device/child22//",
        ]);
        "child_devices_of_child_parent"
    )]
    #[test_case(
        ListFilters::default()
            .parent("device/child2/service/service20".parse().unwrap()), 
        BTreeSet::new();
        "children_of_service_is_empty"
    )]
    #[test_case(
        ListFilters::default()
            .parent("device/child2100//".parse().unwrap()), 
        BTreeSet::new();
        "children_of_leaf_child_is_empty"
    )]
    #[test_case(
        ListFilters::default()
            .root("device/child2100//".parse().unwrap()), 
        BTreeSet::from([
            "device/child2100//",
        ]);
        "entity_tree_from_leaf_child"
    )]
    #[test_case(
        ListFilters::default()
            .root("device/child2/service/service20".parse().unwrap()), 
        BTreeSet::from([
            "device/child2/service/service20",
        ]);
        "entity_tree_from_service"
    )]
    fn list_entity_tree(filters: ListFilters, expected: BTreeSet<&str>) {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        build_test_entity_tree(&mut store);

        // List entity tree from root
        let entities: BTreeSet<&str> = list_entity_tree_topics(&mut store, filters);
        assert_eq!(entities, expected);
    }

    fn list_entity_tree_topics<'a, C: FromIterator<&'a str>>(
        store: &'a mut EntityStore,
        filters: ListFilters,
    ) -> C {
        store
            .list_entity_tree(filters)
            .iter()
            .map(|e| e.topic_id.as_str())
            .collect()
    }
    /// Build the test entity tree:
    ///
    /// main
    /// |-- service0
    /// |-- service1
    /// |-- child0
    /// |   |-- child00
    /// |   |   |-- child000
    /// |-- child1
    /// |   |-- service10
    /// |-- child2
    /// |   |-- service20
    /// |   |-- service21
    /// |   |-- child20
    /// |   |-- child21
    /// |   |   |-- service210
    /// |   |   |-- child210
    /// |   |   |   |-- child2100
    /// |   |   |-- child211
    /// |   |-- child22
    fn build_test_entity_tree(store: &mut EntityStore) {
        build_entity_tree(
            store,
            vec![
                ("device/main/service/service0", "service", None),
                ("device/main/service/service1", "service", None),
                ("device/child0//", "child-device", None),
                ("device/child00//", "child-device", Some("device/child0//")),
                (
                    "device/child000//",
                    "child-device",
                    Some("device/child00//"),
                ),
                ("device/child1//", "child-device", None),
                (
                    "device/child1/service/service10",
                    "service",
                    Some("device/child1//"),
                ),
                ("device/child2//", "child-device", None),
                (
                    "device/child2/service/service20",
                    "service",
                    Some("device/child2//"),
                ),
                (
                    "device/child2/service/service21",
                    "service",
                    Some("device/child2//"),
                ),
                ("device/child20//", "child-device", Some("device/child2//")),
                ("device/child21//", "child-device", Some("device/child2//")),
                (
                    "device/child21/service/service210",
                    "service",
                    Some("device/child21//"),
                ),
                (
                    "device/child210//",
                    "child-device",
                    Some("device/child21//"),
                ),
                (
                    "device/child211//",
                    "child-device",
                    Some("device/child21//"),
                ),
                (
                    "device/child2100//",
                    "child-device",
                    Some("device/child210//"),
                ),
                ("device/child22//", "child-device", Some("device/child2//")),
            ],
        );
    }

    // Each item in the vector represents (topic_id, type, parent)
    fn build_entity_tree(store: &mut EntityStore, entity_tree: Vec<(&str, &str, Option<&str>)>) {
        for entity in entity_tree {
            let topic_id = EntityTopicId::from_str(entity.0).unwrap();
            let r#type = EntityType::from_str(entity.1).unwrap();
            let parent = entity.2.map(|p| EntityTopicId::from_str(p).unwrap());

            store
                .update(EntityRegistrationMessage {
                    topic_id,
                    r#type,
                    external_id: None,
                    parent,
                    health_endpoint: None,
                    twin_data: Map::new(),
                })
                .unwrap();
        }
    }

    #[test]
    fn lists_services() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        // Services are namespaced under devices, so `parent` is not necessary
        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                external_id: None,
                topic_id: EntityTopicId::default_main_service("service1").unwrap(),
                parent: None,
                health_endpoint: None,
                twin_data: Map::new(),
            })
            .unwrap();

        assert_eq!(
            updated_entities.get(0).unwrap().reg_message.topic_id,
            "device/main/service/service1"
        );
        assert_eq!(
            store.services(&EntityTopicId::default_main_device()),
            ["device/main/service/service1"]
        );

        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                external_id: None,
                topic_id: EntityTopicId::default_main_service("service2").unwrap(),
                parent: None,
                health_endpoint: None,
                twin_data: Map::new(),
            })
            .unwrap();

        assert_eq!(
            updated_entities.get(0).unwrap().reg_message.topic_id,
            "device/main/service/service2"
        );
        let services = store.services(&EntityTopicId::default_main_device());
        assert!(services
            .iter()
            .any(|&e| e == &EntityTopicId::default_main_service("service1").unwrap()));
        assert!(services
            .iter()
            .any(|&e| e == &EntityTopicId::default_main_service("service2").unwrap()));
    }

    #[test]
    fn auto_register_service() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let service_topic_id = EntityTopicId::default_child_service("child1", "service1").unwrap();
        let res = store.auto_register_entity(&service_topic_id).unwrap();
        assert_eq!(
            res,
            [
                EntityRegistrationMessage {
                    topic_id: EntityTopicId::from_str("device/child1//").unwrap(),
                    r#type: EntityType::ChildDevice,
                    external_id: None,
                    parent: Some(EntityTopicId::from_str("device/main//").unwrap()),
                    health_endpoint: None,
                    twin_data: json!({ "name": "child1" }).as_object().unwrap().to_owned(),
                },
                EntityRegistrationMessage {
                    topic_id: EntityTopicId::from_str("device/child1/service/service1").unwrap(),
                    r#type: EntityType::Service,
                    external_id: None,
                    parent: Some(EntityTopicId::from_str("device/child1//").unwrap()),
                    health_endpoint: None,
                    twin_data: json!({ "name": "service1" })
                        .as_object()
                        .unwrap()
                        .to_owned(),
                }
            ]
        );
    }

    #[test]
    fn auto_register_child_device() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let child_topic_id = EntityTopicId::default_child_device("child2").unwrap();
        let res = store.auto_register_entity(&child_topic_id).unwrap();

        assert_eq!(
            res,
            [EntityRegistrationMessage {
                topic_id: EntityTopicId::from_str("device/child2//").unwrap(),
                r#type: EntityType::ChildDevice,
                external_id: None,
                parent: Some(EntityTopicId::from_str("device/main//").unwrap()),
                health_endpoint: None,
                twin_data: json!({ "name": "child2" }).as_object().unwrap().to_owned(),
            },]
        );
    }

    #[test]
    fn auto_register_custom_topic_scheme_not_supported() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        assert_matches!(
            store.auto_register_entity(&EntityTopicId::from_str("custom/child2//").unwrap()),
            Err(Error::NonDefaultTopicScheme(_))
        );
    }

    #[test]
    fn register_main_device_custom_scheme() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        // Register main device with custom topic scheme
        let main_topic_id = EntityTopicId::from_str("custom/main//").unwrap();
        store
            .update(EntityRegistrationMessage {
                topic_id: main_topic_id.clone(),
                r#type: EntityType::MainDevice,
                external_id: None,
                parent: None,
                health_endpoint: None,
                twin_data: json!({}).as_object().unwrap().to_owned(),
            })
            .unwrap();

        let expected_entity_metadata =
            EntityMetadata::new(main_topic_id.clone(), EntityType::MainDevice);
        // Assert main device registered with custom topic scheme
        assert_eq!(
            store.get(&main_topic_id).unwrap(),
            &expected_entity_metadata
        );

        // Register service on main device with custom scheme
        let service_topic_id = EntityTopicId::from_str("custom/main/service/collectd").unwrap();
        store
            .update(EntityRegistrationMessage {
                topic_id: service_topic_id.clone(),
                r#type: EntityType::Service,
                external_id: None,
                parent: Some(main_topic_id.clone()),
                health_endpoint: None,
                twin_data: Map::new(),
            })
            .unwrap();

        let expected_entity_metadata =
            EntityMetadata::new(service_topic_id.clone(), EntityType::Service)
                .with_parent(main_topic_id);
        // Assert service registered under main device with custom topic scheme
        assert_eq!(
            store.get(&service_topic_id).unwrap(),
            &expected_entity_metadata
        );
    }

    #[test]
    fn update_twin_data_new_fragment() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let topic_id = EntityTopicId::default_main_device();
        let updated = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "hardware".into(),
                json!({ "version": 5 }),
            ))
            .unwrap();
        assert!(
            updated,
            "Inserting new key should have resulted in an update"
        );

        let entity_metadata = store.get(&topic_id).unwrap();
        assert_eq!(
            entity_metadata.twin_data.get("hardware").unwrap(),
            &json!({ "version": 5 })
        );
    }

    #[test]
    fn update_twin_data_update_existing_fragment() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let topic_id = EntityTopicId::default_main_device();
        let _ = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "hardware".into(),
                json!({ "version": 5 }),
            ))
            .unwrap();

        let updated = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "hardware".into(),
                json!({ "version": 6 }),
            ))
            .unwrap();
        assert!(
            updated,
            "Updating an existing key should have resulted in an update"
        );

        let entity_metadata = store.get(&topic_id).unwrap();
        assert_eq!(
            entity_metadata.twin_data.get("hardware").unwrap(),
            &json!({ "version": 6 })
        );
    }

    #[test]
    fn update_twin_data_remove_fragment() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let topic_id = EntityTopicId::default_main_device();

        let _ = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "foo".into(),
                json!("bar"),
            ))
            .unwrap();

        let updated = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "foo".into(),
                json!(null),
            ))
            .unwrap();
        assert!(
            updated,
            "Removing an existing key should have resulted in an update"
        );

        let entity_metadata = store.get(&topic_id).unwrap();
        assert!(entity_metadata.twin_data.get("foo").is_none());
    }

    #[test]
    fn updated_registration_message_after_twin_updates() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Create an entity store with main device having an explicit `name` fragment
        let topic_id = EntityTopicId::default_main_device();
        let mut store = EntityStore::with_main_device(
            MqttSchema::default(),
            EntityRegistrationMessage {
                topic_id: topic_id.clone(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                health_endpoint: None,
                twin_data: json!({ "name" : "test-name", "type": "test-type" })
                    .as_object()
                    .unwrap()
                    .to_owned(),
            },
            0,
            &temp_dir,
            true,
        )
        .unwrap();

        // Add some additional fragments to the device twin data
        let _ = store
            .update_twin_fragment(EntityTwinMessage::new(
                topic_id.clone(),
                "hardware".into(),
                json!({ "version": 5 }),
            ))
            .unwrap();

        // Update the name of the device with
        let new_reg = EntityRegistrationMessage {
            topic_id: EntityTopicId::default_main_device(),
            external_id: Some("test-device".into()),
            r#type: EntityType::MainDevice,
            parent: None,
            health_endpoint: None,
            twin_data: json!({ "name" : "new-test-device" })
                .as_object()
                .unwrap()
                .to_owned(),
        };
        store.update(new_reg).unwrap();

        // Assert that the old and new twin data are merged
        let entity_metadata = store.get(&topic_id).unwrap();
        assert_eq!(
            entity_metadata.twin_data.get("name").unwrap(),
            &json!("new-test-device"),
            "Expected new name in twin data"
        );
        assert_eq!(
            entity_metadata.twin_data.get("type").unwrap(),
            &json!("test-type"),
            "Expected old type in twin data"
        );
        assert_eq!(
            entity_metadata.twin_data.get("hardware").unwrap(),
            &json!({ "version": 5 }),
            "Expected old hardware fragment in twin data"
        );
    }

    #[test]
    fn duplicate_registration_message_ignored() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, false);
        let entity_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let reg_message = EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            r#type: EntityType::ChildDevice,
            external_id: Some("child1".into()),
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
        };

        let affected_entities = store.update(reg_message.clone()).unwrap();
        assert_eq!(
            affected_entities.get(0).unwrap().reg_message.topic_id,
            "device/child1//"
        );

        let affected_entities = store.update(reg_message.clone()).unwrap();
        assert!(affected_entities.is_empty());

        // Duplicate registration ignore even after the entity store is restored from the disk
        let mut store = new_entity_store(&temp_dir, false);
        let affected_entities = store.update(reg_message).unwrap();
        assert!(affected_entities.is_empty());
    }

    #[test]
    fn duplicate_registration_message_ignored_after_twin_update() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, false);
        let entity_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let reg_message = EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            r#type: EntityType::ChildDevice,
            external_id: Some("child1".into()),
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
        };

        let affected_entities = store.update(reg_message.clone()).unwrap();
        assert_eq!(
            affected_entities.get(0).unwrap().reg_message.topic_id,
            "device/child1//"
        );

        // Update the entity twin data
        store
            .update_twin_fragment(EntityTwinMessage::new(
                entity_topic_id.clone(),
                "foo".into(),
                json!("bar"),
            ))
            .unwrap();

        // Assert that the duplicate registration message is still ignored
        let affected_entities = store.update(reg_message.clone()).unwrap();
        assert!(affected_entities.is_empty());

        // Duplicate registration ignore even after the entity store is restored from the disk
        let mut store = new_entity_store(&temp_dir, false);
        let affected_entities = store.update(reg_message).unwrap();
        assert!(affected_entities.is_empty());
    }

    #[test]
    fn early_child_device_registrations_processed_only_after_parent_registration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let child0_topic_id = EntityTopicId::default_child_device("child0").unwrap();
        let child000_topic_id = EntityTopicId::default_child_device("child000").unwrap();
        let child00_topic_id = EntityTopicId::default_child_device("child00").unwrap();

        // Register great-grand-child before grand-child and child
        let child000_reg_message = EntityRegistrationMessage::new_custom(
            child000_topic_id.clone(),
            EntityType::ChildDevice,
        )
        .with_parent(child00_topic_id.clone());
        let affected_entities = store.update(child000_reg_message.clone()).unwrap();
        assert!(affected_entities.is_empty());

        // Register grand-child before child
        let child00_reg_message = EntityRegistrationMessage::new_custom(
            child00_topic_id.clone(),
            EntityType::ChildDevice,
        )
        .with_parent(child0_topic_id.clone());
        let affected_entities = store.update(child00_reg_message).unwrap();
        assert!(affected_entities.is_empty());

        // Register the immediate child device which will trigger the registration of its children as well
        let child0_reg_message =
            EntityRegistrationMessage::new_custom(child0_topic_id.clone(), EntityType::ChildDevice);
        let affected_entities = store.update(child0_reg_message).unwrap();

        // Assert that the affected entities include all the children
        assert!(!affected_entities.is_empty());

        let affected_entities = store.update(child000_reg_message.clone()).unwrap();
        assert!(affected_entities.is_empty());

        // Reload the entity store from the persistent log
        let mut store = new_entity_store(&temp_dir, true);

        // Assert that duplicate registrations are still ignored
        let affected_entities = store.update(child000_reg_message).unwrap();
        assert!(affected_entities.is_empty());
    }

    #[test]
    fn entities_persisted_and_restored() {
        let temp_dir = tempfile::tempdir().unwrap();

        let child1_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let child2_topic_id = EntityTopicId::default_child_device("child2").unwrap();

        let twin_fragment_key = "foo".to_string();
        let twin_fragment_value = json!("bar");

        {
            let mut store = new_entity_store(&temp_dir, false);
            store
                .update(
                    EntityRegistrationMessage::new_custom(
                        child1_topic_id.clone(),
                        EntityType::ChildDevice,
                    )
                    .with_external_id("child1".into()),
                )
                .unwrap();
            store
                .update_twin_fragment(EntityTwinMessage::new(
                    child1_topic_id.clone(),
                    twin_fragment_key.clone(),
                    twin_fragment_value.clone(),
                ))
                .unwrap();

            store
                .update(
                    EntityRegistrationMessage::new_custom(
                        child2_topic_id.clone(),
                        EntityType::ChildDevice,
                    )
                    .with_external_id("child2".into()),
                )
                .unwrap();
        }

        {
            // Reload the entity store using the same persistent file
            let store = new_entity_store(&temp_dir, false);
            let mut expected_entity_metadata =
                EntityMetadata::child_device("child1".into()).unwrap();
            expected_entity_metadata
                .twin_data
                .insert(twin_fragment_key.clone(), twin_fragment_value.clone());

            let entity_metadata = store.get(&child1_topic_id).unwrap();
            assert_eq!(entity_metadata, &expected_entity_metadata);
            assert_eq!(
                entity_metadata.twin_data.get(&twin_fragment_key).unwrap(),
                &twin_fragment_value
            );

            assert_eq!(
                store.get(&child2_topic_id).unwrap(),
                &EntityMetadata::child_device("child2".into()).unwrap()
            );
        }
    }

    #[test]
    fn deregister_entities() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        register_child(&mut store, "device/main//", "device/001//");
        register_child(&mut store, "device/main//", "device/002//");
        register_child(&mut store, "device/main//", "device/003//");
        register_service(&mut store, "device/main//", "device/main/service/004");

        register_child(&mut store, "device/002//", "device/005//");
        register_child(&mut store, "device/002//", "device/006//");
        register_child(&mut store, "device/002//", "device/007//");
        register_service(&mut store, "device/002//", "device/002/service/008");

        register_child(&mut store, "device/006//", "device/009//");
        register_child(&mut store, "device/006//", "device/00A//");
        register_child(&mut store, "device/006//", "device/00B//");
        register_service(&mut store, "device/006//", "device/006/service/00C");

        register_child(&mut store, "device/003//", "device/00D//");
        register_child(&mut store, "device/003//", "device/00E//");

        let mut removed = store
            .deregister_entity(&entity("device/002//"))
            .into_iter()
            .map(|v| v.topic_id)
            .collect::<Vec<_>>();
        removed.sort_by(|a, b| a.as_str().cmp(b.as_str()));

        assert_eq!(
            removed,
            vec![
                entity("device/002//"),
                entity("device/002/service/008"),
                entity("device/005//"),
                entity("device/006//"),
                entity("device/006/service/00C"),
                entity("device/007//"),
                entity("device/009//"),
                entity("device/00A//"),
                entity("device/00B//"),
            ]
        );

        for topic_id in removed.iter() {
            assert!(store.get(topic_id).is_none());
        }

        assert!(store.get(&entity("device/main//")).is_some());
        assert!(store.get(&entity("device/001//")).is_some());
        assert!(store.get(&entity("device/003//")).is_some());
        assert!(store.get(&entity("device/main/service/004")).is_some());
        assert!(store.get(&entity("device/00D//")).is_some());
        assert!(store.get(&entity("device/00E//")).is_some());
    }

    #[test]
    fn update_parent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        build_entity_tree(
            &mut store,
            vec![
                ("device/child0//", "child-device", None),
                ("device/child00//", "child-device", Some("device/child0//")),
                (
                    "device/child000//",
                    "child-device",
                    Some("device/child00//"),
                ),
                (
                    "device/child001//",
                    "child-device",
                    Some("device/child00//"),
                ),
                ("device/child01//", "child-device", Some("device/child0//")),
                ("device/child1//", "child-device", None),
                ("device/child10//", "child-device", Some("device/child1//")),
            ],
        );

        // Assert sub-trees of `child0` and `child1` before the update
        assert_eq!(
            list_entity_tree_topics::<Vec<&str>>(
                &mut store,
                ListFilters::default().root("device/child0//".parse().unwrap()),
            ),
            [
                "device/child0//",
                "device/child00//",
                "device/child01//",
                "device/child000//",
                "device/child001//",
            ]
        );
        assert_eq!(
            list_entity_tree_topics::<Vec<&str>>(
                &mut store,
                ListFilters::default().root("device/child1//".parse().unwrap()),
            ),
            ["device/child1//", "device/child10//"]
        );

        let entity = store
            .update_entity(
                &"device/child00//".parse().unwrap(),
                EntityUpdateMessage::default().with_parent("device/child1//".parse().unwrap()),
            )
            .unwrap();

        let expected =
            EntityMetadata::new("device/child00//".parse().unwrap(), EntityType::ChildDevice)
                .with_parent("device/child1//".parse().unwrap());
        assert_eq!(entity, &expected);

        // Assert sub-trees of `child0` and `child1` after the update
        assert_eq!(
            list_entity_tree_topics::<Vec<&str>>(
                &mut store,
                ListFilters::default().root("device/child0//".parse().unwrap()),
            ),
            ["device/child0//", "device/child01//"]
        );
        assert_eq!(
            list_entity_tree_topics::<Vec<&str>>(
                &mut store,
                ListFilters::default().root("device/child1//".parse().unwrap()),
            ),
            [
                "device/child1//",
                "device/child00//",
                "device/child10//",
                "device/child000//",
                "device/child001//",
            ]
        );
    }

    #[test_case(
        "device/child0//",
        "device/child0//",
        "Entity: 'device/child0//' can not be its own parent";
        "invalid_self_parent"
    )]
    #[test_case(
        "device/main//",
        "device/child0//",
        "The parent of main device can not be updated";
        "immutable_main_device"
    )]
    #[test_case(
        "device/child0//",
        "device/main/service/tedge-agent",
        "Entity: 'device/main/service/tedge-agent' can not be a parent of 'device/child0//' because it is a service. Only devices can be parents";
        "invalid_service_parent"
    )]
    #[test_case(
        "device/child0//",
        "device/child000//",
        "Entity: 'device/child000//' can not be a parent of 'device/child0//' because 'device/child000//' is a descendent of 'device/child0//'";
        "invalid_descendent_parent"
    )]
    fn invalid_update_parent(topic_id: &str, new_parent: &str, error_msg: &str) {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        build_entity_tree(
            &mut store,
            vec![
                (
                    "device/main/service/tedge-agent",
                    "service",
                    Some("device/main//"),
                ),
                ("device/child0//", "child-device", None),
                ("device/child00//", "child-device", Some("device/child0//")),
                (
                    "device/child000//",
                    "child-device",
                    Some("device/child00//"),
                ),
            ],
        );

        let entity_topic_id = EntityTopicId::from_str(topic_id).unwrap();
        let update_message = EntityUpdateMessage::default()
            .with_parent(EntityTopicId::from_str(new_parent).unwrap());

        assert_eq!(
            store
                .update_entity(&entity_topic_id, update_message)
                .unwrap_err()
                .to_string(),
            error_msg
        );
    }

    #[test]
    fn update_health_endpoint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        store
            .update(
                EntityRegistrationMessage::main_device(None)
                    .with_health_endpoint("device/main/service/tedge-agent".parse().unwrap()),
            )
            .unwrap();
        store
            .update(EntityRegistrationMessage::new_custom(
                "health-service".parse().unwrap(),
                EntityType::Service,
            ))
            .unwrap();

        let entity = store
            .update_entity(
                &EntityTopicId::default_main_device(),
                EntityUpdateMessage::default()
                    .with_health_endpoint("health-service/".parse().unwrap()),
            )
            .unwrap();

        let expected = EntityMetadata::main_device(None)
            .with_health_endpoint("health-service///".parse().unwrap());
        assert_eq!(entity, &expected);
    }

    #[test]
    fn invalid_update_health_endpoint() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        let child_device = EntityTopicId::default_child_device("child0").unwrap();
        store
            .update(EntityRegistrationMessage::new_custom(
                child_device.clone(),
                EntityType::ChildDevice,
            ))
            .unwrap();

        let update_message = EntityUpdateMessage::default().with_health_endpoint(child_device);

        assert_matches!(
            store.update_entity(&EntityTopicId::default_main_device(), update_message),
            Err(Error::InvalidHealthEndpoint(_, _))
        );
    }

    #[test_case(
        "device/child000//",
        vec!["device/child00//", "device/child0//", "device/main//"];
        "leaf_node"
    )]
    #[test_case(
        "device/child00//",
        vec!["device/child0//", "device/main//"];
        "nested_child"
    )]
    #[test_case(
        "device/child0//",
        vec!["device/main//"];
        "immediate_child"
    )]
    #[test_case(
        "device/main//",
        vec![];
        "main_device"
    )]
    fn ancestors(topic_id: &str, expected: Vec<&str>) {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);
        build_entity_tree(
            &mut store,
            vec![
                (
                    "device/main/service/tedge-agent",
                    "service",
                    Some("device/main//"),
                ),
                ("device/child0//", "child-device", None),
                ("device/child00//", "child-device", Some("device/child0//")),
                (
                    "device/child000//",
                    "child-device",
                    Some("device/child00//"),
                ),
                (
                    "device/child000/service/service0",
                    "service",
                    Some("device/child000//"),
                ),
            ],
        );
        let ancestors: Vec<&str> = store
            .ancestors(&EntityTopicId::from_str(topic_id).unwrap())
            .unwrap()
            .iter()
            .map(|e| e.as_str())
            .collect();

        assert_eq!(ancestors, expected);
    }

    fn new_entity_store(temp_dir: &TempDir, clean_start: bool) -> EntityStore {
        EntityStore::with_main_device(
            MqttSchema::default(),
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                health_endpoint: None,
                twin_data: Map::new(),
            },
            0,
            temp_dir,
            clean_start,
        )
        .unwrap()
    }

    fn register(store: &mut EntityStore, topic_id: &str, payload: JsonValue) {
        let registration = EntityRegistrationMessage::try_from(
            topic_id.parse().unwrap(),
            payload.to_string().as_bytes(),
        );
        store.update(registration.unwrap()).unwrap();
        assert!(store.get(&entity(topic_id)).is_some());
    }

    fn register_child(store: &mut EntityStore, parent: &str, topic_id: &str) {
        register(
            store,
            topic_id,
            json!({"@type": "child-device", "@parent": parent}),
        )
    }

    fn register_service(store: &mut EntityStore, parent: &str, topic_id: &str) {
        register(
            store,
            topic_id,
            json!({"@type": "service", "@parent": parent}),
        )
    }

    fn entity(topic_id: &str) -> EntityTopicId {
        EntityTopicId::from_str(topic_id).unwrap()
    }
}
