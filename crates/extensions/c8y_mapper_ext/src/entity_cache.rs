use std::collections::hash_map::Entry;
use std::collections::BTreeSet;
use std::collections::HashMap;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::EntityType;
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
    #[error("The specified entity {0} does not exist in the entity cache")]
    UnknownEntity(String),

    #[error(transparent)]
    InvalidExternalId(#[from] InvalidExternalIdError),

    #[error("Updating the external id of {0} is not supported")]
    InvalidExternalIdUpdate(EntityTopicId),

    #[error("Updating the entity type of {0} is not supported")]
    InvalidEntityTypeUpdate(EntityTopicId),

    #[error("Updating the twin data of {0} with a registration message is not supported")]
    InvalidEntityTwinUpdate(EntityTopicId),

    #[error("Updating the parent of the main device is not supported")]
    InvalidMainDeviceParentUpdate,

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Unchanged,
    Inserted(Vec<RegisteredEntityData>),
    Updated(Box<EntityMetadata>, Box<EntityMetadata>),
    Deleted,
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
        let main_device_metadata = CloudEntityMetadata::new(
            main_device_xid.clone(),
            EntityMetadata::main_device(Some(main_device_xid.clone())),
        );

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

    /// Insert a new entity or update an existing entity
    ///
    /// Return Inserted if the entity is new. Any pending child entities of the given entity are also registered and returned.
    /// Return Updated if the entity was previously registered and has been updated by this call
    /// Return Unchanged if the entity not affected by this call
    pub fn upsert(&mut self, entity: EntityRegistrationMessage) -> Result<UpdateOutcome, Error> {
        let topic_id = entity.topic_id.clone();
        let previous = self.entities.entry(entity.topic_id.clone());
        let outcome = match previous {
            Entry::Occupied(current) => {
                let existing_entity = current.get().metadata.clone();
                self.update(entity, existing_entity)?
            }
            Entry::Vacant(_) => {
                if !self.insert(entity.clone())? {
                    return Ok(UpdateOutcome::Unchanged);
                }

                let current_entity_data = self.pending_entities.take_cached_entity_data(entity);
                let mut registered_entities = vec![current_entity_data];

                let pending_children = self
                    .pending_entities
                    .take_cached_child_entities_data(&topic_id);
                for pending_child in pending_children {
                    let child_reg_message = pending_child.reg_message.clone();
                    self.insert(child_reg_message)?;
                    registered_entities.push(pending_child);
                }

                UpdateOutcome::Inserted(registered_entities)
            }
        };

        debug!("Updated entity map: {:?}", self.entities);
        Ok(outcome)
    }

    fn insert(&mut self, entity: EntityRegistrationMessage) -> Result<bool, Error> {
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

        if entity.r#type != EntityType::MainDevice
            && !self.entities.contains_key(
                parent
                    .as_ref()
                    .expect("At least a default parent exists for child entities"),
            )
        {
            self.pending_entities
                .cache_early_registration_message(entity);
            return Ok(false);
        }

        let topic_id = entity.topic_id.clone();
        let external_id = if let Some(id) = entity.external_id {
            (self.external_id_validator_fn)(id.as_ref())?
        } else if entity.r#type == EntityType::MainDevice {
            self.main_device_xid.clone()
        } else {
            (self.external_id_mapper_fn)(&entity.topic_id, self.main_device_external_id())
        };

        let entity_metadata = EntityMetadata {
            topic_id: topic_id.clone(),
            external_id: Some(external_id.clone()),
            r#type: entity.r#type,
            parent,
            health_endpoint: entity.health_endpoint,
            twin_data: entity.twin_data,
            persistent_channels: BTreeSet::new(),
        };

        self.entities.insert(
            topic_id.clone(),
            CloudEntityMetadata::new(external_id.clone(), entity_metadata),
        );
        self.external_id_map.insert(external_id, topic_id);

        Ok(true)
    }

    fn update(
        &mut self,
        entity: EntityRegistrationMessage,
        existing_entity: EntityMetadata,
    ) -> Result<UpdateOutcome, Error> {
        let topic_id = entity.topic_id.clone();

        if entity.r#type != existing_entity.r#type {
            return Err(Error::InvalidEntityTypeUpdate(topic_id.clone()));
        }

        if entity.external_id.is_some() && entity.external_id != existing_entity.external_id {
            return Err(Error::InvalidExternalIdUpdate(topic_id.clone()));
        }

        if entity.r#type == EntityType::MainDevice && entity.parent != existing_entity.parent {
            return Err(Error::InvalidMainDeviceParentUpdate);
        }

        let mut merged_twin_data = existing_entity.twin_data.clone();
        merged_twin_data.extend(entity.twin_data);
        if merged_twin_data != existing_entity.twin_data {
            return Err(Error::InvalidEntityTwinUpdate(topic_id.clone()));
        }

        let updated_entity = EntityMetadata {
            topic_id: topic_id.clone(),
            external_id: existing_entity.external_id.clone(),
            r#type: existing_entity.r#type.clone(),
            parent: entity
                .parent
                .clone()
                .or_else(|| existing_entity.parent.clone()),
            health_endpoint: entity
                .health_endpoint
                .clone()
                .or_else(|| existing_entity.health_endpoint.clone()),
            twin_data: existing_entity.twin_data.clone(),
            persistent_channels: BTreeSet::new(),
        };

        if existing_entity == updated_entity {
            return Ok(UpdateOutcome::Unchanged);
        }
        self.entities.insert(
            topic_id.clone(),
            CloudEntityMetadata::new(
                updated_entity
                    .external_id
                    .clone()
                    .expect("External id must be present"),
                updated_entity.clone(),
            ),
        );
        Ok(UpdateOutcome::Updated(
            Box::new(updated_entity),
            Box::new(existing_entity),
        ))
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
    use crate::entity_cache::CloudEntityMetadata;
    use crate::entity_cache::UpdateOutcome;
    use assert_matches::assert_matches;
    use serde_json::Map;
    use tedge_api::entity::EntityExternalId;
    use tedge_api::entity::EntityMetadata;
    use tedge_api::entity::EntityType;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;

    #[test]
    fn external_id_generation() {
        let mut cache = new_entity_cache();

        let topic_id: EntityTopicId = "device/child1//".parse().unwrap();
        let res = cache.upsert(EntityRegistrationMessage::new_custom(
            topic_id.clone(),
            EntityType::ChildDevice,
        ));
        assert_matches!(res, Ok(UpdateOutcome::Inserted(_)));
        let external_id: EntityExternalId = "test-device:device:child1".into();
        let expected = CloudEntityMetadata::new(
            external_id.clone(),
            EntityMetadata::new(topic_id.clone(), EntityType::ChildDevice)
                .with_external_id(external_id)
                .with_parent(EntityTopicId::default_main_device()),
        );
        assert_eq!(cache.get(&topic_id), Some(&expected));
    }

    #[test]
    fn external_id_validation() {
        let mut cache = new_entity_cache();

        let entity_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let res = cache.upsert(EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            r#type: EntityType::ChildDevice,
            external_id: Some("bad+id".into()),
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
        });

        // Assert service registered under main device with custom topic scheme
        assert_matches!(res, Err(Error::InvalidExternalId(_)));
    }

    #[test]
    fn main_device_health_endpoint_update() {
        let mut cache = new_entity_cache();

        let res = cache.upsert(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_main_device(),
                EntityType::MainDevice,
            )
            .with_health_endpoint(EntityTopicId::default_main_service("foo").unwrap()),
        );
        assert_matches!(res, Ok(UpdateOutcome::Updated(_, _)));
        if let UpdateOutcome::Updated(new, old) = res.unwrap() {
            assert_eq!(
                new.health_endpoint,
                Some(EntityTopicId::default_main_service("foo").unwrap())
            );
            assert_eq!(new.external_id, Some("test-device".into())); //Kept from the original registration
            assert_eq!(old.health_endpoint, None);
        }
    }

    #[test]
    fn main_device_twin_update_with_reg_message_not_supported() {
        let mut cache = new_entity_cache();

        let res = cache.upsert(
            EntityRegistrationMessage::new_custom(
                EntityTopicId::default_main_device(),
                EntityType::MainDevice,
            )
            .with_twin_fragment("new".to_string(), "fragment".into()),
        );

        assert_matches!(res, Err(Error::InvalidEntityTwinUpdate(_)));
    }

    #[test]
    fn subset_reg_message_does_not_update() {
        let mut cache = new_entity_cache();

        let topic_id = EntityTopicId::default_child_device("child0").unwrap();
        let reg_message =
            EntityRegistrationMessage::new_custom(topic_id.clone(), EntityType::ChildDevice)
                .with_external_id("child0".into())
                .with_parent(EntityTopicId::default_main_device())
                .with_twin_fragment("key1".to_string(), "val1".into())
                .with_twin_fragment("key2".to_string(), "val2".into())
                .with_twin_fragment("key3".to_string(), "val3".into());
        let res = cache.upsert(reg_message.clone());
        assert_matches!(res, Ok(UpdateOutcome::Inserted(_)));

        // Same reg message
        let res = cache.upsert(reg_message);
        assert_matches!(res, Ok(UpdateOutcome::Unchanged));

        // Reg message without the original twin data
        let res = cache.upsert(EntityRegistrationMessage::new_custom(
            topic_id.clone(),
            EntityType::ChildDevice,
        ));
        assert_matches!(res, Ok(UpdateOutcome::Unchanged));

        // Reg message with a subset of original twin data
        let res = cache.upsert(
            EntityRegistrationMessage::new_custom(topic_id.clone(), EntityType::ChildDevice)
                .with_twin_fragment("key2".to_string(), "val2".into()),
        );
        assert_matches!(res, Ok(UpdateOutcome::Unchanged));

        let metadata = cache.get(&topic_id).unwrap();
        assert_eq!(metadata.external_id, "child0".into());
        assert_eq!(metadata.metadata.external_id, Some("child0".into()));
        assert_eq!(
            metadata.metadata.parent,
            Some(EntityTopicId::default_main_device())
        );
        assert_eq!(metadata.metadata.twin_data.len(), 3);
        assert_eq!(metadata.metadata.twin_data["key1"], "val1");
        assert_eq!(metadata.metadata.twin_data["key2"], "val2");
        assert_eq!(metadata.metadata.twin_data["key3"], "val3");
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
