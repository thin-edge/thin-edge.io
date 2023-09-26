//! A store containing registered MQTT entities.
//!
//! References:
//!
//! - <https://github.com/thin-edge/thin-edge.io/issues/2081>
//! - <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#entity-store>

// TODO: move entity business logic to its own module

use std::collections::HashMap;

use crate::entity_store;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::TopicIdError;
use mqtt_channel::Message;
use serde_json::json;

/// Represents an "Entity topic identifier" portion of the MQTT topic
///
/// Example:
/// - topic: `te/device/dev1/service/myservice/m//my_measurement`
/// - entity id: `device/dev1/service/myservice`
///
/// <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-identifier>

// In the future, root will be read from config
const MQTT_ROOT: &str = "te";

/// Represents externally provided unique ID of an entity.
/// Although this struct doesn't enforce any restrictions for the values,
/// the consumers may impose restrictions on the accepted values.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct EntityExternalId(String);

impl AsRef<str> for EntityExternalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<&str> for EntityExternalId {
    fn from(val: &str) -> Self {
        Self(val.to_string())
    }
}

impl From<String> for EntityExternalId {
    fn from(val: String) -> Self {
        Self(val)
    }
}

impl From<EntityExternalId> for String {
    fn from(value: EntityExternalId) -> Self {
        value.0
    }
}

type ExternalIdMapperFn =
    Box<dyn Fn(&EntityTopicId, &EntityExternalId) -> EntityExternalId + Send + Sync + 'static>;

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
/// # use mqtt_channel::{Message, Topic};
/// # use tedge_api::entity_store::{EntityStore, EntityRegistrationMessage};
/// let mqtt_message = Message::new(
///     &Topic::new("te/device/main//").unwrap(),
///     r#"{"@type": "device"}"#.to_string(),
/// );
/// let registration_message = EntityRegistrationMessage::try_from(&mqtt_message).unwrap();
///
/// let mut entity_store = EntityStore::with_main_device(registration_message, |tid, xid| tid.to_string().into());
/// ```
pub struct EntityStore {
    main_device: EntityTopicId,
    entities: HashMap<EntityTopicId, EntityMetadata>,
    entity_id_index: HashMap<EntityExternalId, EntityTopicId>,
    external_id_mapper: ExternalIdMapperFn,
}

impl EntityStore {
    /// Creates a new entity store with a given main device.
    #[must_use]
    pub fn with_main_device<F>(
        main_device: EntityRegistrationMessage,
        external_id_mapper: F,
    ) -> Option<Self>
    where
        F: Fn(&EntityTopicId, &EntityExternalId) -> EntityExternalId,
        F: 'static + Send + Sync,
    {
        if main_device.r#type != EntityType::MainDevice {
            return None;
        }

        let entity_id: EntityExternalId = main_device.external_id?;
        let metadata = EntityMetadata {
            topic_id: main_device.topic_id.clone(),
            external_id: entity_id.clone(),
            r#type: main_device.r#type,
            parent: None,
            other: main_device.payload,
        };

        Some(EntityStore {
            main_device: main_device.topic_id.clone(),
            entities: HashMap::from([(main_device.topic_id.clone(), metadata)]),
            entity_id_index: HashMap::from([(entity_id, main_device.topic_id)]),
            external_id_mapper: Box::new(external_id_mapper),
        })
    }

    /// Returns information about an entity under a given MQTT entity topic identifier.
    pub fn get(&self, entity_topic_id: &EntityTopicId) -> Option<&EntityMetadata> {
        self.entities.get(entity_topic_id)
    }

    /// Returns information for an entity under a given device/service id .
    pub fn get_by_external_id(&self, external_id: &EntityExternalId) -> Option<&EntityMetadata> {
        let topic_id = self.entity_id_index.get(external_id)?;
        self.get(topic_id)
    }

    /// Returns the MQTT identifier of the main device.
    ///
    /// The main device is an entity with `@type: "device"`.
    pub fn main_device(&self) -> &EntityTopicId {
        &self.main_device
    }

    /// Returns the external id of the main device.
    pub fn main_device_external_id(&self) -> EntityExternalId {
        self.get(&self.main_device).unwrap().external_id.clone()
    }

    /// Returns an ordered list of ancestors of the given entity
    /// starting from the immediate parent all the way till the root main device.
    /// The last parent in the list for any entity would always be the main device.
    /// The list would be empty for the main device as it has no further parents.
    pub fn ancestors(&self, entity_topic_id: &EntityTopicId) -> Result<Vec<&EntityTopicId>, Error> {
        if self.entities.get(entity_topic_id).is_none() {
            return Err(Error::UnknownEntity(entity_topic_id.to_string()));
        }

        let mut ancestors = vec![];

        let mut current_entity_id = entity_topic_id;
        while let Some(entity) = self.entities.get(current_entity_id) {
            if let Some(parent_id) = &entity.parent {
                ancestors.push(parent_id);
                current_entity_id = parent_id;
            } else {
                break; // No more parents
            }
        }

        Ok(ancestors)
    }

    /// Returns an ordered list of ancestors' external ids of the given entity
    /// starting from the immediate parent all the way till the root main device.
    /// The last parent in the list for any entity would always be the main device id.
    pub fn ancestors_external_ids(
        &self,
        entity_topic_id: &EntityTopicId,
    ) -> Result<Vec<String>, Error> {
        let mapped_ancestors = self
            .ancestors(entity_topic_id)?
            .iter()
            .map(|tid| {
                self.entities
                    .get(tid)
                    .map(|e| e.external_id.clone().into())
                    .unwrap()
            })
            .collect();

        Ok(mapped_ancestors)
    }

    /// Returns MQTT identifiers of child devices of a given device.
    pub fn child_devices(&self, entity_topic: &EntityTopicId) -> Vec<&EntityTopicId> {
        self.entities
            .iter()
            .filter(|(_, e)| {
                // can be replaced by `is_some_and` after MSRV upgrade to 1.70
                e.parent.as_ref().map_or(false, |p| p == entity_topic)
                    && e.r#type == EntityType::ChildDevice
            })
            .map(|(k, _)| k)
            .collect()
    }

    /// Returns MQTT identifiers of services running on a given device.
    pub fn services(&self, entity_topic: &EntityTopicId) -> Vec<&EntityTopicId> {
        self.entities
            .iter()
            .filter(|(_, e)| {
                // can be replaced by `is_some_and` after MSRV upgrade to 1.70
                e.parent.as_ref().map_or(false, |p| p == entity_topic)
                    && e.r#type == EntityType::Service
            })
            .map(|(k, _)| k)
            .collect()
    }

    /// Updates entity store state based on the content of the entity
    /// registration message.
    ///
    /// It can register a new entity in the store or update already registered
    /// entity, returning a list of all entities affected by the update, e.g.:
    ///
    /// - when adding/removing a child device or service, the parent is affected
    pub fn update(
        &mut self,
        message: EntityRegistrationMessage,
    ) -> Result<Vec<EntityTopicId>, Error> {
        let topic_id = message.topic_id;

        let mut affected_entities = vec![];

        let parent = match message.r#type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => message.parent.or_else(|| Some(self.main_device.clone())),
            EntityType::Service => message
                .parent
                .or_else(|| topic_id.default_parent_identifier())
                .or_else(|| Some(self.main_device.clone())),
        };

        // parent device is affected if new device is its child
        if let Some(parent) = &parent {
            if !self.entities.contains_key(parent) {
                return Err(Error::NoParent(parent.to_string().into_boxed_str()));
            }

            affected_entities.push(parent.clone());
        }

        let external_id = match message.r#type {
            EntityType::MainDevice => self.main_device_external_id(),
            _ => message.external_id.unwrap_or_else(|| {
                (self.external_id_mapper)(&topic_id, &self.main_device_external_id())
            }),
        };
        let entity_metadata = EntityMetadata {
            topic_id: topic_id.clone(),
            r#type: message.r#type,
            external_id: external_id.clone(),
            parent,
            other: message.payload,
        };

        // device is affected if it was previously registered and was updated
        let previous = self
            .entities
            .insert(entity_metadata.topic_id.clone(), entity_metadata);

        if previous.is_some() {
            affected_entities.push(topic_id);
        } else {
            self.entity_id_index.insert(external_id, topic_id);
        }

        Ok(affected_entities)
    }

    /// An iterator over all registered entities.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityTopicId, &EntityMetadata)> {
        self.entities.iter()
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
        if entity_topic_id.matches_default_topic_scheme() {
            if entity_topic_id.is_default_main_device() {
                return Ok(vec![]); // Do nothing as the main device is always pre-registered
            }

            let mut register_messages = vec![];

            let parent_device_id = entity_topic_id
                .default_parent_identifier()
                .expect("device id must be present as the topic id follows the default scheme");

            if !parent_device_id.is_default_main_device() && self.get(&parent_device_id).is_none() {
                let device_local_id = entity_topic_id.default_device_name().unwrap();
                let device_external_id =
                    (self.external_id_mapper)(&parent_device_id, &self.main_device_external_id());

                let device_register_message = EntityRegistrationMessage {
                    topic_id: parent_device_id.clone(),
                    external_id: Some(device_external_id),
                    r#type: EntityType::ChildDevice,
                    parent: None,
                    payload: json!({ "name": device_local_id }),
                };
                register_messages.push(device_register_message.clone());
                self.update(device_register_message)?;
            }

            // if the entity is a service, register the service as well
            if let Some(service_id) = entity_topic_id.default_service_name() {
                let service_external_id =
                    (self.external_id_mapper)(entity_topic_id, &self.main_device_external_id());

                let service_register_message = EntityRegistrationMessage {
                    topic_id: entity_topic_id.clone(),
                    external_id: Some(service_external_id),
                    r#type: EntityType::Service,
                    parent: Some(parent_device_id),
                    payload: json!({ "name": service_id,  "type": "systemd" }),
                };
                register_messages.push(service_register_message.clone());
                self.update(service_register_message)?;
            }

            Ok(register_messages)
        } else {
            Err(Error::NonDefaultTopicScheme(entity_topic_id.clone()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    pub topic_id: EntityTopicId,
    pub parent: Option<EntityTopicId>,
    pub r#type: EntityType,
    pub external_id: EntityExternalId,
    pub other: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityType {
    MainDevice,
    ChildDevice,
    Service,
}

impl EntityMetadata {
    /// Creates a entity metadata for the main device.
    pub fn main_device(device_id: String) -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: device_id.into(),
            r#type: EntityType::MainDevice,
            parent: None,
            other: serde_json::json!({}),
        }
    }

    /// Creates a entity metadata for a child device.
    pub fn child_device(child_device_id: String) -> Result<Self, TopicIdError> {
        Ok(Self {
            topic_id: EntityTopicId::default_child_device(&child_device_id)?,
            external_id: child_device_id.into(),
            r#type: EntityType::ChildDevice,
            parent: Some(EntityTopicId::default_main_device()),
            other: serde_json::json!({}),
        })
    }
}

/// Represents an error encountered while updating the store.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("Specified parent {0:?} does not exist in the store")]
    NoParent(Box<str>),

    #[error("Main device was not registered. Before registering child entities, register the main device")]
    NoMainDevice,

    #[error("The main device was already registered at topic {0}")]
    MainDeviceAlreadyRegistered(Box<str>),

    #[error("The specified entity {0} does not exist in the store")]
    UnknownEntity(String),

    #[error("The specified topic id {0} does not match the default topic scheme: 'device/<device-id>/service/<service-id>'")]
    NonDefaultTopicScheme(EntityTopicId),
}

/// An object representing a valid entity registration message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRegistrationMessage {
    pub topic_id: EntityTopicId,
    pub external_id: Option<EntityExternalId>,
    pub r#type: EntityType,
    pub parent: Option<EntityTopicId>,
    pub payload: serde_json::Value,
}

impl EntityRegistrationMessage {
    /// Parses a MQTT message as an entity registration message.
    ///
    /// MQTT message is an entity registration message if
    /// - published on a prefix of `te/+/+/+/+`
    /// - its payload contains a registration message.
    #[must_use]
    pub fn new(message: &Message) -> Option<Self> {
        let payload = parse_entity_register_payload(message.payload_bytes())?;

        let r#type = payload
            .get("@type")
            .and_then(|t| t.as_str())
            .map(|t| t.to_owned())?;
        let r#type = match r#type.as_str() {
            "device" => EntityType::MainDevice,
            "child-device" => EntityType::ChildDevice,
            "service" => EntityType::Service,
            _ => return None,
        };

        let parent = if r#type == EntityType::ChildDevice || r#type == EntityType::Service {
            payload
                .get("@parent")
                .and_then(|p| p.as_str())
                .and_then(|p| p.parse().ok())
        } else {
            None
        };

        let entity_id = payload
            .get("@id")
            .and_then(|id| id.as_str())
            .map(|id| id.into());

        let topic_id = message
            .topic
            .name
            .strip_prefix(MQTT_ROOT)
            .and_then(|s| s.strip_prefix('/'))?;

        Some(Self {
            topic_id: topic_id.parse().ok()?,
            external_id: entity_id,
            r#type,
            parent,
            payload,
        })
    }

    /// Creates a entity registration message for a main device.
    pub fn main_device(main_device_id: String) -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: Some(main_device_id.into()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: serde_json::json!({}),
        }
    }
}

impl TryFrom<&Message> for EntityRegistrationMessage {
    type Error = ();

    fn try_from(value: &Message) -> Result<Self, Self::Error> {
        EntityRegistrationMessage::new(value).ok_or(())
    }
}

/// Parse a MQTT message payload as an entity registration payload.
///
/// Returns `Some(register_payload)` if a payload is valid JSON and is a
/// registration payload, or `None` otherwise.
fn parse_entity_register_payload(payload: &[u8]) -> Option<serde_json::Value> {
    let payload = serde_json::from_slice::<serde_json::Value>(payload).ok()?;

    if payload.get("@type").is_some() {
        Some(payload)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use assert_matches::assert_matches;
    use mqtt_channel::Topic;
    use serde_json::json;

    use super::*;

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

    #[test]
    fn registers_main_device() {
        let store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                payload: json!({"@type": "device"}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        assert_eq!(store.main_device(), &EntityTopicId::default_main_device());
        assert!(store.get(&EntityTopicId::default_main_device()).is_some());
    }

    #[test]
    fn lists_child_devices() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                payload: json!({"@type": "device"}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        // If the @parent info is not provided, it is assumed to be an immediate
        // child of the main device.
        let updated_entities = store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child1//").unwrap(),
                    json!({"@type": "child-device"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        assert_eq!(updated_entities, ["device/main//"]);
        assert_eq!(
            store.child_devices(&EntityTopicId::default_main_device()),
            ["device/child1//"]
        );

        let updated_entities = store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child2//").unwrap(),
                    json!({"@type": "child-device", "@parent": "device/main//"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();
        assert_eq!(updated_entities, ["device/main//"]);
        let children = store.child_devices(&EntityTopicId::default_main_device());
        assert!(children.iter().any(|&e| e == "device/child1//"));
        assert!(children.iter().any(|&e| e == "device/child2//"));
    }

    #[test]
    fn lists_services() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                r#type: EntityType::MainDevice,
                external_id: Some("test-device".into()),
                topic_id: EntityTopicId::default_main_device(),
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        // Services are namespaced under devices, so `parent` is not necessary
        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                external_id: None,
                topic_id: EntityTopicId::default_main_service("service1").unwrap(),
                parent: None,
                payload: json!({}),
            })
            .unwrap();

        assert_eq!(updated_entities, ["device/main//"]);
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
                payload: json!({}),
            })
            .unwrap();

        assert_eq!(updated_entities, ["device/main//"]);
        let services = store.services(&EntityTopicId::default_main_device());
        assert!(services
            .iter()
            .any(|&e| e == &EntityTopicId::default_main_service("service1").unwrap()));
        assert!(services
            .iter()
            .any(|&e| e == &EntityTopicId::default_main_service("service2").unwrap()));
    }

    #[test]
    fn forbids_nonexistent_parents() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        let res = store.update(EntityRegistrationMessage {
            topic_id: EntityTopicId::default_main_device(),
            external_id: None,
            r#type: EntityType::ChildDevice,
            parent: Some(EntityTopicId::default_child_device("myawesomeparent").unwrap()),
            payload: json!({}),
        });

        assert!(matches!(res, Err(Error::NoParent(_))));
    }

    #[test]
    fn list_ancestors() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                payload: json!({"@type": "device"}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        // Assert no ancestors of main device
        assert!(store
            .ancestors(&EntityTopicId::default_main_device())
            .unwrap()
            .is_empty());

        // Register service on main
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/main/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestors of main device service
        assert_eq!(
            store
                .ancestors(&EntityTopicId::default_main_service("collectd").unwrap())
                .unwrap(),
            ["device/main//"]
        );

        // Register immediate child of main
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child1//").unwrap(),
                    json!({"@type": "child-device"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestors of child1
        assert_eq!(
            store
                .ancestors(&EntityTopicId::default_child_device("child1").unwrap())
                .unwrap(),
            ["device/main//"]
        );

        // Register service on child1
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child1/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestors of child1 service
        assert_eq!(
            store
                .ancestors(&EntityTopicId::default_child_service("child1", "collectd").unwrap())
                .unwrap(),
            ["device/child1//", "device/main//"]
        );

        // Register child2 as child of child1
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child2//").unwrap(),
                    json!({"@type": "child-device", "@parent": "device/child1//"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestors of child2
        assert_eq!(
            store
                .ancestors(&EntityTopicId::default_child_device("child2").unwrap())
                .unwrap(),
            ["device/child1//", "device/main//"]
        );

        // Register service on child2
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child2/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestors of child2 service
        assert_eq!(
            store
                .ancestors(&EntityTopicId::default_child_service("child2", "collectd").unwrap())
                .unwrap(),
            ["device/child2//", "device/child1//", "device/main//"]
        );
    }

    #[test]
    fn list_ancestors_external_ids() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                external_id: Some("test-device".into()),
                r#type: EntityType::MainDevice,
                parent: None,
                payload: json!({"@type": "device"}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        // Assert ancestor external ids of main device
        assert!(store
            .ancestors_external_ids(&EntityTopicId::default_main_device())
            .unwrap()
            .is_empty());

        // Register service on main
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/main/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestor external id of main device service
        assert_eq!(
            store
                .ancestors_external_ids(&EntityTopicId::default_main_service("collectd").unwrap())
                .unwrap(),
            ["test-device"]
        );

        // Register immediate child of main
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child1//").unwrap(),
                    json!({"@type": "child-device"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestor external ids of child1
        assert_eq!(
            store
                .ancestors_external_ids(&EntityTopicId::default_child_device("child1").unwrap())
                .unwrap(),
            ["test-device"]
        );

        // Register service on child1
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child1/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestor external ids of child1 service
        assert_eq!(
            store
                .ancestors_external_ids(
                    &EntityTopicId::default_child_service("child1", "collectd").unwrap()
                )
                .unwrap(),
            ["device:child1", "test-device"]
        );

        // Register child2 as child of child1
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child2//").unwrap(),
                    json!({"@type": "child-device", "@parent": "device/child1//"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestor external ids of child2
        assert_eq!(
            store
                .ancestors_external_ids(&EntityTopicId::default_child_device("child2").unwrap())
                .unwrap(),
            ["device:child1", "test-device"]
        );

        // Register service on child2
        store
            .update(
                EntityRegistrationMessage::new(&Message::new(
                    &Topic::new("te/device/child2/service/collectd").unwrap(),
                    json!({"@type": "service"}).to_string(),
                ))
                .unwrap(),
            )
            .unwrap();

        // Assert ancestor external ids of child2 service
        assert_eq!(
            store
                .ancestors_external_ids(
                    &EntityTopicId::default_child_service("child2", "collectd").unwrap()
                )
                .unwrap(),
            ["device:child2", "device:child1", "test-device"]
        );
    }

    #[test]
    fn auto_register_service() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                r#type: EntityType::MainDevice,
                external_id: Some("test-device".into()),
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        let service_topic_id = EntityTopicId::default_child_service("child1", "service1").unwrap();
        let res = store.auto_register_entity(&service_topic_id).unwrap();
        assert_eq!(
            res,
            [
                EntityRegistrationMessage {
                    topic_id: EntityTopicId::from_str("device/child1//").unwrap(),
                    r#type: EntityType::ChildDevice,
                    external_id: Some("device:child1".into()),
                    parent: None,
                    payload: json!({ "name": "child1" }),
                },
                EntityRegistrationMessage {
                    topic_id: EntityTopicId::from_str("device/child1/service/service1").unwrap(),
                    r#type: EntityType::Service,
                    external_id: Some("device:child1:service:service1".into()),
                    parent: Some(EntityTopicId::from_str("device/child1//").unwrap()),
                    payload: json!({ "name": "service1",  "type": "systemd" }),
                }
            ]
        );
    }

    #[test]
    fn auto_register_child_device() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                r#type: EntityType::MainDevice,
                external_id: Some("test-device".into()),
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        let child_topic_id = EntityTopicId::default_child_device("child2").unwrap();
        let res = store.auto_register_entity(&child_topic_id).unwrap();

        assert_eq!(
            res,
            [EntityRegistrationMessage {
                topic_id: EntityTopicId::from_str("device/child2//").unwrap(),
                r#type: EntityType::ChildDevice,
                external_id: Some("device:child2".into()),
                parent: None,
                payload: json!({ "name": "child2" }),
            },]
        );
    }

    #[test]
    fn auto_register_custom_topic_scheme_not_supported() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                r#type: EntityType::MainDevice,
                external_id: Some("test-device".into()),
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();
        assert_matches!(
            store.auto_register_entity(&EntityTopicId::from_str("custom/child2//").unwrap()),
            Err(Error::NonDefaultTopicScheme(_))
        );
    }

    #[test]
    fn register_main_device_custom_scheme() {
        let mut store = EntityStore::with_main_device(
            EntityRegistrationMessage {
                topic_id: EntityTopicId::default_main_device(),
                r#type: EntityType::MainDevice,
                external_id: Some("test-device".into()),
                parent: None,
                payload: json!({}),
            },
            dummy_external_id_mapper,
        )
        .unwrap();

        // Register main device with custom topic scheme
        let main_topic_id = EntityTopicId::from_str("custom/main//").unwrap();
        store
            .update(EntityRegistrationMessage {
                topic_id: main_topic_id.clone(),
                r#type: EntityType::MainDevice,
                external_id: None,
                parent: None,
                payload: json!({}),
            })
            .unwrap();

        let expected_entity_metadata = EntityMetadata {
            topic_id: main_topic_id.clone(),
            parent: None,
            r#type: EntityType::MainDevice,
            external_id: "test-device".into(),
            other: json!({}),
        };
        // Assert main device registered with custom topic scheme
        assert_eq!(
            store.get(&main_topic_id).unwrap(),
            &expected_entity_metadata
        );
        assert_eq!(
            store.get_by_external_id(&"test-device".into()).unwrap(),
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
                payload: json!({}),
            })
            .unwrap();

        let expected_entity_metadata = EntityMetadata {
            topic_id: service_topic_id.clone(),
            parent: Some(main_topic_id),
            r#type: EntityType::Service,
            external_id: "custom:main:service:collectd".into(),
            other: json!({}),
        };
        // Assert service registered under main device with custom topic scheme
        assert_eq!(
            store.get(&service_topic_id).unwrap(),
            &expected_entity_metadata
        );
        assert_eq!(
            store
                .get_by_external_id(&"custom:main:service:collectd".into())
                .unwrap(),
            &expected_entity_metadata
        );
    }
}
