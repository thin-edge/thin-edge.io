use std::collections::HashMap;
use tedge_api::entity::EntityExternalId;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::entity_store::EntityType;
use tedge_api::mqtt_topics::default_topic_schema;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::pending_entity_store::PendingEntityData;
use tedge_api::pending_entity_store::PendingEntityStore;
use tedge_mqtt_ext::MqttMessage;
use thiserror::Error;

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

pub(crate) struct EntityCache {
    main_device_tid: EntityTopicId,
    main_device_xid: EntityExternalId,
    external_id_mapper: ExternalIdMapperFn,
    external_id_validator_fn: ExternalIdValidatorFn,

    entities: HashMap<EntityTopicId, EntityMetadata>,
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
        let main_device_metadata = EntityMetadata {
            topic_id: main_device_tid.clone(),
            external_id: main_device_xid.clone(),
            r#type: EntityType::MainDevice,
            parent: None,
            display_name: Some(main_device_xid.clone().into()),
            display_type: Some("thin-edge.io".into()),
        };

        Self {
            main_device_xid: main_device_xid.clone(),
            main_device_tid: main_device_tid.clone(),
            entities: HashMap::from([(main_device_tid.clone(), main_device_metadata)]),
            external_id_map: HashMap::from([(main_device_xid, main_device_tid)]),
            pending_entities: PendingEntityStore::new(mqtt_schema, telemetry_cache_size),
            external_id_mapper: Box::new(external_id_mapper_fn),
            external_id_validator_fn: Box::new(external_id_validator_fn),
        }
    }

    pub(crate) fn register_entity(
        &mut self,
        entity: EntityRegistrationMessage,
    ) -> Result<Vec<PendingEntityData>, InvalidExternalIdError> {
        let parent = entity.parent.as_ref().unwrap_or(&self.main_device_tid);
        if self.entities.contains_key(parent) {
            self.register_single_entity(entity.clone())?;

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
    ) -> Result<(), InvalidExternalIdError> {
        let external_id = if let Some(id) = entity.external_id {
            (self.external_id_validator_fn)(id.as_ref())?
        } else {
            (self.external_id_mapper)(&entity.topic_id, self.main_device_external_id())
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
            external_id: external_id.clone(),
            r#type: entity.r#type,
            parent,
            display_name: entity
                .other
                .get("name")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            display_type: entity
                .other
                .get("type")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
        };

        self.entities
            .insert(entity.topic_id.clone(), entity_metadata);
        self.external_id_map.insert(external_id, entity.topic_id);

        Ok(())
    }

    pub(crate) fn get_entity_metadata_by_external_id(
        &self,
        topic_id: &EntityExternalId,
    ) -> Option<&EntityMetadata> {
        self.external_id_map
            .get(topic_id)
            .and_then(|topic_id| self.entities.get(topic_id))
    }

    /// Returns information about an entity under a given MQTT entity topic identifier.
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entities.get(entity_topic_id)
    }

    /// Tries to get information about an entity using its `EntityTopicId`,
    /// returning an error if the entity is not registered.
    pub fn try_get(&self, entity_topic_id: &EntityTopicId) -> Result<&EntityMetadata, Error> {
        self.get(entity_topic_id)
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
    ) -> Result<&EntityMetadata, Error> {
        self.get_entity_metadata_by_external_id(external_id)
            .ok_or_else(|| Error::UnknownEntity(external_id.into()))
    }

    /// Returns the external id of the main device.
    pub fn main_device_topic_id(&self) -> &EntityTopicId {
        &self.main_device_tid
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
        let parent_xid = entity.parent.as_ref().map(|tid| {
            &self
                .try_get(tid)
                .expect(
                    "for every registered entity, its parent is also guaranteed to be registered",
                )
                .external_id
        });

        Ok(parent_xid)
    }

    pub fn cache_early_data_message(&mut self, message: MqttMessage) {
        self.pending_entities.cache_early_data_message(message)
    }

    // TODO: Temporarily placed here. To be removed when the agent handles auto registration
    pub fn auto_register_entity(
        &mut self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<Vec<EntityRegistrationMessage>, Error> {
        let auto_entities = default_topic_schema::parse(entity_topic_id);
        if auto_entities.is_empty() {
            return Err(Error::NonDefaultTopicScheme(entity_topic_id.clone()));
        };

        let mut register_messages = vec![];
        for mut auto_entity in auto_entities {
            // Skip any already registered entity
            if auto_entity.r#type != EntityType::MainDevice
                && self.get(&auto_entity.topic_id).is_none()
            {
                if auto_entity.r#type == EntityType::Service {
                    auto_entity
                        .other
                        .insert("type".to_string(), "service".into());
                }

                register_messages.push(auto_entity.clone());
                self.register_entity(auto_entity)?;
            }
        }

        Ok(register_messages)
    }
}

#[cfg(FALSE)]
mod tests {
    #[test]
    fn external_id_validation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut store = new_entity_store(&temp_dir, true);

        let entity_topic_id = EntityTopicId::default_child_device("child1").unwrap();
        let res = store.update(EntityRegistrationMessage {
            topic_id: entity_topic_id.clone(),
            r#type: EntityType::ChildDevice,
            external_id: Some("bad+id".into()),
            parent: None,
            other: Map::new(),
        });

        // Assert service registered under main device with custom topic scheme
        assert_matches!(res, Err(Error::InvalidExternalIdError(_)));
    }

    fn dummy_external_id_mapper(
        entity_topic_id: &EntityTopicId,
        _main_device_xid: &EntityExternalId,
    ) -> EntityExternalId {
        entity_topic_id
            .to_string()
            .trim_end_matches('/')
            .replace('/', ":")
            .into()
    }

    fn dummy_external_id_sanitizer(id: &str) -> Result<EntityExternalId, InvalidExternalIdError> {
        let forbidden_chars = HashSet::from(['/', '+', '#']);
        for c in id.chars() {
            if forbidden_chars.contains(&c) {
                return Err(InvalidExternalIdError {
                    external_id: id.into(),
                    invalid_char: c,
                });
            }
        }
        Ok(id.into())
    }
}
