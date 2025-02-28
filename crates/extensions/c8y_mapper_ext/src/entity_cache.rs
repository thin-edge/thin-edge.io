use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::EntityType;
use tedge_api::entity::InsertOutcome;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityTwinMessage;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::pending_entity_store::PendingEntityStore;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_mqtt_ext::MqttMessage;
use thiserror::Error;
use tracing::debug;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[error("Invalid external ID: {external_id} contains invalid character: {invalid_char}")]
pub struct InvalidExternalIdError {
    pub external_id: String,
    pub invalid_char: char,
}

type ExternalIdMapperFn =
    Box<dyn Fn(&EntityTopicId, &EntityExternalId) -> EntityExternalId + Send + Sync + 'static>;
type ExternalIdValidatorFn =
    Box<dyn Fn(&str) -> Result<EntityExternalId, InvalidExternalIdError> + Send + Sync + 'static>;

/// Represents an error encountered while updating the store.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("The specified entity {0} does not exist in the store")]
    UnknownEntity(String),

    #[error(transparent)]
    InvalidExternalId(#[from] InvalidExternalIdError),

    #[error("Auto registration of the entity with topic id {0} failed as it does not match the default topic scheme: 'device/<device-id>/service/<service-id>'. Try explicit registration instead.")]
    NonDefaultTopicScheme(EntityTopicId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudEntityMetadata {
    pub external_id: EntityExternalId,
    pub metadata: EntityMetadata,
}

impl CloudEntityMetadata {
    pub fn new(external_id: EntityExternalId, metadata: EntityMetadata) -> Self {
        Self {
            external_id,
            metadata,
        }
    }
}

/// An in-memory cache of entity metadata with their external ids, indexed by their entity topic ids.
/// The external id is the unique identifier of the entity twin in the connected cloud instance.
/// This id is used
/// Each entity in this cache is a mirror of the same entity in the entity store maintained by the agent,
/// with the addition of the external id.
///
/// Every entity registered in this cache would have an external id which is either specified as the `@id`
/// when the entity is registered or auto derived from the entity topic id.
/// The user provided ids are validated using the `external_id_validator_fn` before they are added to the cache.
/// When an `@id` is not provided, one is generated using the `external_id_mapper_fn`.
///
/// Any entity that is registered before its parents are cached in the `pending_entities` store,
/// until those parents are registered as well.
/// Once the parent is registered, the pending child entities are also registered along with it.
pub(crate) struct EntityCache {
    main_device_tid: EntityTopicId,
    main_device_xid: EntityExternalId,
    external_id_mapper_fn: ExternalIdMapperFn,
    external_id_validator_fn: ExternalIdValidatorFn,

    entities: HashMap<EntityTopicId, CloudEntityMetadata>,
    external_id_map: HashMap<EntityExternalId, EntityTopicId>,
    pub pending_entities: PendingEntityStore,
}

impl EntityCache {
    pub(crate) fn new<MF, SF>(
        mqtt_schema: MqttSchema,
        main_device_tid: EntityTopicId,
        main_device_xid: EntityExternalId,
        external_id_mapper_fn: MF,
        external_id_validator_fn: SF,
        telemetry_cache_size: usize,
    ) -> Self
    where
        MF: Fn(&EntityTopicId, &EntityExternalId) -> EntityExternalId,
        MF: 'static + Send + Sync,
        SF: Fn(&str) -> Result<EntityExternalId, InvalidExternalIdError>,
        SF: 'static + Send + Sync,
    {
        let main_device_metadata =
            CloudEntityMetadata::new(main_device_xid.clone(), EntityMetadata::main_device());

        Self {
            main_device_xid: main_device_xid.clone(),
            main_device_tid: main_device_tid.clone(),
            entities: HashMap::from([(main_device_tid.clone(), main_device_metadata)]),
            external_id_map: HashMap::from([(main_device_xid, main_device_tid)]),
            pending_entities: PendingEntityStore::new(mqtt_schema, telemetry_cache_size),
            external_id_mapper_fn: Box::new(external_id_mapper_fn),
            external_id_validator_fn: Box::new(external_id_validator_fn),
        }
    }

    pub(crate) fn register_entity(
        &mut self,
        entity: EntityRegistrationMessage,
    ) -> Result<Vec<RegisteredEntityData>, Error> {
        let parent = entity.parent.as_ref().unwrap_or(&self.main_device_tid);
        if self.entities.contains_key(parent) {
            let outcome = self.register_single_entity(entity.clone())?;
            if outcome == InsertOutcome::Unchanged {
                return Ok(vec![]);
            }

            let topic_id = entity.topic_id.clone();
            let current_entity_data = self
                .pending_entities
                .take_cached_entity_data(entity.clone());
            let mut pending_entities = vec![current_entity_data];

            let pending_children = self
                .pending_entities
                .take_cached_child_entities_data(&topic_id);
            for pending_child in pending_children {
                let child_reg_message = pending_child.reg_message.clone();
                self.register_single_entity(child_reg_message)?;
                pending_entities.push(pending_child);
            }
            Ok(pending_entities)
        } else {
            self.pending_entities
                .cache_early_registration_message(entity);
            Ok(vec![])
        }
    }

    pub fn register_single_entity(
        &mut self,
        entity: EntityRegistrationMessage,
    ) -> Result<InsertOutcome, InvalidExternalIdError> {
        let external_id = if let Some(id) = entity.external_id {
            (self.external_id_validator_fn)(id.as_ref())?
        } else {
            (self.external_id_mapper_fn)(&entity.topic_id, self.main_device_external_id())
        };

        let parent = match entity.r#type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => entity
                .parent
                .clone()
                .or_else(|| Some(self.main_device_tid.clone())),
            EntityType::Service => entity
                .parent
                .clone()
                .or_else(|| entity.topic_id.default_service_parent_identifier())
                .or_else(|| Some(self.main_device_tid.clone())),
        };

        let entity_metadata = EntityMetadata {
            topic_id: entity.topic_id.clone(),
            external_id: Some(external_id.clone()),
            r#type: entity.r#type,
            parent,
            twin_data: entity.twin_data,
        };

        let outcome = self.insert(entity.topic_id.clone(), external_id, entity_metadata);

        Ok(outcome)
    }

    /// Insert a new entity
    ///
    /// Return Inserted if the entity is new
    /// Return Updated if the entity was previously registered and has been updated by this call
    /// Return Unchanged if the entity not affected by this call
    fn insert(
        &mut self,
        topic_id: EntityTopicId,
        external_id: EntityExternalId,
        entity_metadata: EntityMetadata,
    ) -> InsertOutcome {
        let previous = self.entities.entry(topic_id.clone());
        let outcome = match previous {
            Entry::Occupied(mut occupied) => {
                // if there is no change, no entities were affected
                let existing_entity = &occupied.get().metadata;

                let mut merged_other = existing_entity.twin_data.clone();
                merged_other.extend(entity_metadata.twin_data.clone());
                let merged_entity = EntityMetadata {
                    twin_data: merged_other,
                    ..entity_metadata
                };

                if existing_entity == &merged_entity {
                    InsertOutcome::Unchanged
                } else {
                    occupied.insert(CloudEntityMetadata::new(external_id.clone(), merged_entity));
                    InsertOutcome::Updated
                }
            }
            Entry::Vacant(vacant) => {
                vacant.insert(CloudEntityMetadata::new(
                    external_id.clone(),
                    entity_metadata,
                ));
                InsertOutcome::Inserted
            }
        };
        self.external_id_map.insert(external_id, topic_id);

        debug!("Updated entity map: {:?}", self.entities);
        outcome
    }

    pub(crate) fn delete(&mut self, topic_id: &EntityTopicId) -> Option<CloudEntityMetadata> {
        let entity = self.entities.remove(topic_id);
        if let Some(entity) = &entity {
            self.external_id_map.remove(&entity.external_id);
        }
        entity
    }

    pub fn update_twin_data(&mut self, twin_message: EntityTwinMessage) -> Result<bool, Error> {
        let fragment_key = twin_message.fragment_key.clone();
        let fragment_value = twin_message.fragment_value.clone();
        let entity = self.try_get_mut(&twin_message.topic_id)?;
        if fragment_value.is_null() {
            let existing = entity.metadata.twin_data.remove(&fragment_key);
            if existing.is_none() {
                return Ok(false);
            }
        } else {
            let existing = entity
                .metadata
                .twin_data
                .insert(fragment_key, fragment_value.clone());
            if existing.is_some_and(|v| v.eq(&fragment_value)) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub(crate) fn get_entity_metadata_by_external_id(
        &self,
        topic_id: &EntityExternalId,
    ) -> Option<&CloudEntityMetadata> {
        self.external_id_map
            .get(topic_id)
            .and_then(|topic_id| self.entities.get(topic_id))
    }

    /// Returns information about an entity under a given MQTT entity topic identifier.
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&CloudEntityMetadata> {
        self.entities.get(entity_topic_id)
    }

    /// Returns a mutable reference to the `EntityMetadata` for the given `EntityTopicId`.
    fn get_mut(&mut self, entity_topic_id: &EntityTopicId) -> Option<&mut CloudEntityMetadata> {
        self.entities.get_mut(entity_topic_id)
    }

    /// Tries to get information about an entity using its `EntityTopicId`,
    /// returning an error if the entity is not registered.
    pub fn try_get(&self, entity_topic_id: &EntityTopicId) -> Result<&CloudEntityMetadata, Error> {
        self.get(entity_topic_id)
            .ok_or_else(|| Error::UnknownEntity(entity_topic_id.to_string()))
    }

    /// Tries to get a mutable reference to the `EntityMetadata` for the given `EntityTopicId`,
    /// returning an error if the entity is not registered.
    fn try_get_mut(
        &mut self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<&mut CloudEntityMetadata, Error> {
        self.get_mut(entity_topic_id)
            .ok_or_else(|| Error::UnknownEntity(entity_topic_id.to_string()))
    }

    pub(crate) fn try_get_external_id(
        &self,
        topic_id: &EntityTopicId,
    ) -> Result<&EntityExternalId, Error> {
        self.try_get(topic_id).map(|e| &e.external_id)
    }

    /// Tries to get information about an entity using its `EntityExternalId`,
    /// returning an error if the entity is not registered.
    pub fn try_get_by_external_id(
        &self,
        external_id: &EntityExternalId,
    ) -> Result<&CloudEntityMetadata, Error> {
        self.get_entity_metadata_by_external_id(external_id)
            .ok_or_else(|| Error::UnknownEntity(external_id.into()))
    }

    /// Returns the external id of the main device.
    pub fn main_device_external_id(&self) -> &EntityExternalId {
        &self.main_device_xid
    }

    /// Returns the external id of the parent of the given entity.
    /// Returns None for the main device, that doesn't have any parents.
    pub fn parent_external_id(
        &self,
        entity_tid: &EntityTopicId,
    ) -> Result<Option<&EntityExternalId>, Error> {
        let entity = self.try_get(entity_tid)?;
        let parent_xid = entity.metadata.parent.as_ref().map(|tid| {
            self.try_get_external_id(tid).expect(
                "For every registered entity, its parent is also guaranteed to be registered",
            )
        });

        Ok(parent_xid)
    }

    pub fn cache_early_data_message(&mut self, message: MqttMessage) {
        self.pending_entities.cache_early_data_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::EntityCache;
    use super::Error;
    use crate::converter::CumulocityConverter;
    use assert_matches::assert_matches;
    use serde_json::Map;
    use tedge_api::entity::EntityType;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;

    #[test]
    fn external_id_validation() {
        let mut cache = new_entity_cache();

        let entity_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let res = cache.register_entity(EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            r#type: EntityType::ChildDevice,
            external_id: Some("bad+id".into()),
            parent: None,
            twin_data: Map::new(),
        });

        // Assert service registered under main device with custom topic scheme
        assert_matches!(res, Err(Error::InvalidExternalId(_)));
    }

    fn new_entity_cache() -> EntityCache {
        EntityCache::new(
            MqttSchema::default(),
            EntityTopicId::default_main_device(),
            "test-device".into(),
            CumulocityConverter::map_to_c8y_external_id,
            CumulocityConverter::validate_external_id,
            10,
        )
    }
}
