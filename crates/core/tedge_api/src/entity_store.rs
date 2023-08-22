//! A store containing registered MQTT entities.
//!
//! References:
//!
//! - <https://github.com/thin-edge/thin-edge.io/issues/2081>
//! - <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#entity-store>

// TODO: move entity business logic to its own module

use std::collections::HashMap;

use crate::entity::EntityTopic;
use crate::entity_store;
use mqtt_channel::Message;
use mqtt_channel::Topic;

/// Represents an "Entity topic identifier" portion of the MQTT topic
///
/// Example:
/// - topic: `te/device/dev1/service/myservice/m//my_measurement`
/// - entity id: `device/dev1/service/myservice`
///
/// <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-identifier>
type EntityTopicId = String;

// type checker is not able to infer `&EntityTopicId` as &str, so alias is needed
//
// TODO: try using Rc, Cow, or string interning to get rid of duplicate strings
type EntityTopicIdRef<'a> = &'a str;

// In the future, root will be read from config
const MQTT_ROOT: &str = "te";

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
/// let mut entity_store = EntityStore::with_main_device(registration_message);
/// ```
#[derive(Debug, Clone)]
pub struct EntityStore {
    main_device: EntityTopicId,
    entities: HashMap<EntityTopicId, EntityMetadata>,
    entity_id_index: HashMap<String, EntityTopicId>,
}

impl EntityStore {
    /// Creates a new entity store with a given main device.
    #[must_use]
    pub fn with_main_device(main_device: EntityRegistrationMessage) -> Option<Self> {
        if main_device.r#type != EntityType::MainDevice {
            return None;
        }

        let entity_id = main_device.entity_id?;
        let metadata = EntityMetadata {
            entity_id: entity_id.clone(),
            r#type: main_device.r#type,
            parent: None,
            other: main_device.payload,
        };

        Some(EntityStore {
            main_device: main_device.topic_id.clone(),
            entities: HashMap::from([(main_device.topic_id.clone(), metadata)]),
            entity_id_index: HashMap::from([(entity_id, main_device.topic_id)]),
        })
    }

    /// Returns information about an entity under a given MQTT entity topic.
    pub fn get(&self, entity_id: &str) -> Option<&EntityMetadata> {
        self.entities.get(entity_id)
    }

    /// Returns information for an entity under a given external id.
    pub fn get_by_id(&self, entity_id: &str) -> Option<&EntityMetadata> {
        let topic_id = self.entity_id_index.get(entity_id)?;
        self.get(topic_id)
    }

    /// Returns the entity attached to a topic, if any
    pub fn get_entity_from_topic(&self, topic: &Topic) -> Option<&EntityMetadata> {
        let entity_topic = EntityTopic::try_from(topic).ok()?;
        self.get(entity_topic.entity_id())
    }

    /// Returns the MQTT identifier of the main device.
    ///
    /// The main device is an entity with `@type: "device"`.
    pub fn main_device(&self) -> EntityTopicIdRef {
        self.main_device.as_str()
    }

    /// Returns the name of main device.
    pub fn main_device_name(&self) -> &str {
        self.get(self.main_device.as_str())
            .unwrap()
            .entity_id
            .as_str()
    }

    /// Returns MQTT identifiers of child devices of a given device.
    pub fn child_devices(&self, entity_topic: EntityTopicIdRef) -> Vec<EntityTopicIdRef> {
        self.entities
            .iter()
            .filter(|(_, e)| {
                // can be replaced by `is_some_and` after MSRV upgrade to 1.70
                e.parent.as_ref().map_or(false, |p| p == entity_topic)
                    && e.r#type == EntityType::ChildDevice
            })
            .map(|(k, _)| k.as_str())
            .collect()
    }

    /// Returns MQTT identifiers of services running on a given device.
    pub fn services(&self, entity_topic: EntityTopicIdRef) -> Vec<EntityTopicIdRef> {
        self.entities
            .iter()
            .filter(|(_, e)| {
                // can be replaced by `is_some_and` after MSRV upgrade to 1.70
                e.parent.as_ref().map_or(false, |p| p == entity_topic)
                    && e.r#type == EntityType::Service
            })
            .map(|(k, _)| k.as_str())
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
        if message.r#type == EntityType::MainDevice && message.topic_id != self.main_device {
            return Err(Error::MainDeviceAlreadyRegistered(
                self.main_device.as_str().into(),
            ));
        }

        let mut affected_entities = vec![];

        let parent = if message.r#type == EntityType::MainDevice {
            None
        } else {
            message.parent.or(Some(self.main_device.clone()))
        };

        // parent device is affected if new device is its child
        if let Some(parent) = &parent {
            if !self.entities.contains_key(parent) {
                return Err(Error::NoParent(parent.clone().into_boxed_str()));
            }

            affected_entities.push(parent.clone());
        }

        let entity_id = message
            .entity_id
            .unwrap_or_else(|| self.derive_entity_id(&message.topic_id));
        let entity_metadata = EntityMetadata {
            r#type: message.r#type,
            entity_id: entity_id.clone(),
            parent,
            other: message.payload,
        };

        // device is affected if it was previously registered and was updated
        let previous = self
            .entities
            .insert(message.topic_id.clone(), entity_metadata);

        if previous.is_some() {
            affected_entities.push(message.topic_id);
        } else {
            self.entity_id_index.insert(entity_id, message.topic_id);
        }

        Ok(affected_entities)
    }

    /// An iterator over all registered entities.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityTopicId, &EntityMetadata)> {
        self.entities.iter()
    }

    /// Generate child device external ID.
    ///
    /// The external id is generated by prefixing the id with main device name
    /// (device_common_name) and then appending the MQTT entity topic with `/`
    ///  characters replaced by `:`.
    ///
    /// # Examples
    /// - `device/main//` => `DEVICE_COMMON_NAME`
    /// - `device/child001//` => `DEVICE_COMMON_NAME:device:child001`
    /// - `device/child001/service/service001` => `DEVICE_COMMON_NAME:device:child001:service:service001`
    /// - `factory01/hallA/packaging/belt001` => `DEVICE_COMMON_NAME:factory01:hallA:packaging:belt001`
    fn derive_entity_id(&self, entity_topic: EntityTopicIdRef) -> String {
        if entity_topic == self.main_device {
            self.get(&self.main_device).unwrap().entity_id.to_string()
        } else {
            let main_device_entity_id = &self.get(&self.main_device).unwrap().entity_id;
            let entity_id_suffix = entity_topic.replace('/', ":");
            let entity_id_suffix = entity_id_suffix.trim_matches(':');

            format!("{main_device_entity_id}:{entity_id_suffix}")
        }
    }

    /// Performs auto-registration process for an entity under a given
    /// identifier.
    ///
    /// If an entity is a service, its device is also auto-registered if it's
    /// not already registered.
    ///
    /// It returns MQTT register messages for the given entities to be published
    /// by the mapper, so other components can also be aware of a new device
    /// being registered.
    pub fn auto_register_entity(
        &mut self,
        entity_id: &str,
    ) -> Result<Vec<Message>, entity_store::Error> {
        let mut register_messages = vec![];
        let (device_id, service_id) = match entity_id.split('/').collect::<Vec<&str>>()[..] {
            ["device", device_id, "service", service_id, ..] => (device_id, Some(service_id)),
            ["device", device_id, "", ""] => (device_id, None),
            _ => return Ok(register_messages),
        };

        // register device if not registered
        let device_topic = format!("{MQTT_ROOT}/device/{device_id}//");
        if self.get(&device_topic).is_none() {
            let device_type = if device_id == "main" {
                "device"
            } else {
                "child-device"
            };
            let device_name = if device_id == "main" {
                self.main_device_name()
            } else {
                device_id
            };
            let device_register_payload =
                format!("{{ \"@type\":\"{device_type}\", \"@id\":\"{device_name}\"}}");
            let device_register_message =
                Message::new(&Topic::new(&device_topic).unwrap(), device_register_payload)
                    .with_retain();
            register_messages.push(device_register_message.clone());
            self.update(EntityRegistrationMessage::try_from(&device_register_message).unwrap())?;
        }

        // register service itself
        if let Some(service_id) = service_id {
            let service_topic = format!("{MQTT_ROOT}/device/{device_id}/service/{service_id}");
            let service_register_payload = r#"{"@type": "service", "type": "systemd"}"#.to_string();
            let service_register_message = Message::new(
                &Topic::new(&service_topic).unwrap(),
                service_register_payload,
            )
            .with_retain();
            register_messages.push(service_register_message.clone());
            self.update(EntityRegistrationMessage::try_from(&service_register_message).unwrap())?;
        }

        Ok(register_messages)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    pub parent: Option<EntityTopicId>,
    pub r#type: EntityType,
    pub entity_id: String,
    pub other: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityType {
    MainDevice,
    ChildDevice,
    Service,
}

impl EntityMetadata {
    /// Creates a entity metadata for a child device.
    pub fn main_device(device_id: String) -> Self {
        Self {
            entity_id: device_id,
            r#type: EntityType::MainDevice,
            parent: None,
            other: serde_json::json!({}),
        }
    }

    /// Creates a entity metadata for a child device.
    pub fn child_device(child_device_id: String) -> Self {
        Self {
            entity_id: child_device_id,
            r#type: EntityType::ChildDevice,
            parent: Some("device/main//".to_string()),
            other: serde_json::json!({}),
        }
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
}

/// An object representing a valid entity registration message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRegistrationMessage {
    topic_id: EntityTopicId,
    entity_id: Option<String>,
    r#type: EntityType,
    parent: Option<EntityTopicId>,
    payload: serde_json::Value,
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
                .map(|p| p.to_owned())
        } else {
            None
        };

        let entity_id = payload
            .get("@id")
            .and_then(|id| id.as_str())
            .map(|id| id.to_string());

        let topic_id = message
            .topic
            .name
            .strip_prefix(MQTT_ROOT)
            .and_then(|s| s.strip_prefix('/'))?;

        Some(Self {
            topic_id: topic_id.to_string(),
            entity_id,
            r#type,
            parent,
            payload,
        })
    }

    /// Creates a entity registration message for a main device.
    pub fn main_device(entity_id: String) -> Self {
        Self {
            topic_id: "device/main//".to_string(),
            entity_id: Some(entity_id),
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

/// Extracts an MQTT entity identifier from an MQTT topic.
///
/// This function is usually used for obtaining an entity MQTT identifier from a
/// command or telemetry topic of this entity.
///
/// The MQTT topic has to contain `root` and `identifier` groups described in
/// [thin-edge documentation on MQTT topic scheme](https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#topic-scheme).
/// If these groups are not present, the function returns `None`.
///
/// ```
/// # use mqtt_channel::Topic;
/// # use tedge_api::entity_store::entity_topic_id;
/// let entity_measurement_topic = Topic::new("te/device/main/service/my_service/m/my_measurement").unwrap();
/// assert_eq!(entity_topic_id(&entity_measurement_topic), Some("device/main/service/my_service"));

/// let custom_topic = Topic::new("te/device/1/2/3/m/my_measurement").unwrap();
/// assert_eq!(entity_topic_id(&custom_topic), Some("device/1/2/3"));
///
/// let custom_topic = Topic::new("custom_root/device/1/2/3/m/my_measurement").unwrap();
/// assert_eq!(entity_topic_id(&custom_topic), None);
/// ```
// TODO: this should be moved to MQTT parsing module when it's created
// https://github.com/thin-edge/thin-edge.io/pull/2118#issuecomment-1668110422
pub fn entity_topic_id(topic: &Topic) -> Option<&str> {
    let topic = topic
        .name
        .strip_prefix(MQTT_ROOT)
        .and_then(|s| s.strip_prefix('/'))?;

    let identifier_len = topic
        .match_indices('/')
        .nth(3)
        .map_or(topic.len(), |(i, _)| i);

    Some(&topic[..identifier_len])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn registers_main_device() {
        let store = EntityStore::with_main_device(EntityRegistrationMessage {
            topic_id: "device/main//".to_string(),
            entity_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({"@type": "device"}),
        })
        .unwrap();

        assert_eq!(store.main_device(), "device/main//");
        assert!(store.get("device/main//").is_some());
    }

    #[test]
    fn lists_child_devices() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            topic_id: "device/main//".to_string(),
            entity_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({"@type": "device"}),
        })
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
        assert_eq!(store.child_devices("device/main//"), ["device/child1//"]);

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
        let children = store.child_devices("device/main//");
        assert!(children.iter().any(|&e| e == "device/child1//"));
        assert!(children.iter().any(|&e| e == "device/child2//"));
    }

    #[test]
    fn lists_services() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            r#type: EntityType::MainDevice,
            entity_id: Some("test-device".to_string()),
            topic_id: "device/main//".to_string(),
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        // Services are namespaced under devices, so `parent` is not necessary
        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                entity_id: None,
                topic_id: "device/main/service/service1".to_string(),
                parent: None,
                payload: json!({}),
            })
            .unwrap();

        assert_eq!(updated_entities, ["device/main//"]);
        assert_eq!(
            store.services("device/main//"),
            ["device/main/service/service1"]
        );

        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                entity_id: None,
                topic_id: "device/main/service/service2".to_string(),
                parent: None,
                payload: json!({}),
            })
            .unwrap();

        assert_eq!(updated_entities, ["device/main//"]);
        let services = store.services("device/main//");
        assert!(services
            .iter()
            .any(|&e| e == "device/main/service/service1"));
        assert!(services
            .iter()
            .any(|&e| e == "device/main/service/service2"));
    }

    /// Forbids creating multiple main devices.
    ///
    /// Publishing new registration message on a topic where main device is
    /// registered updates the main device and is allowed. Creating a new main
    /// device on another topic is not allowed.
    #[test]
    fn forbids_multiple_main_devices() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            topic_id: "device/main//".try_into().unwrap(),
            r#type: EntityType::MainDevice,
            entity_id: Some("test-device".to_string()),
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        let res = store.update(EntityRegistrationMessage {
            topic_id: "device/another_main//".try_into().unwrap(),
            entity_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({}),
        });

        assert_eq!(
            res,
            Err(Error::MainDeviceAlreadyRegistered("device/main//".into()))
        );
    }

    #[test]
    fn forbids_nonexistent_parents() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            topic_id: "device/main//".try_into().unwrap(),
            entity_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        let res = store.update(EntityRegistrationMessage {
            topic_id: "device/main//".try_into().unwrap(),
            entity_id: None,
            r#type: EntityType::ChildDevice,
            parent: Some("device/myawesomeparent//".to_string()),
            payload: json!({}),
        });

        assert!(matches!(res, Err(Error::NoParent(_))));
    }

    #[test]
    fn generates_entity_ids() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            topic_id: "device/main//".try_into().unwrap(),
            entity_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        store
            .update(EntityRegistrationMessage {
                topic_id: "device/child001/service/service001".to_string(),
                entity_id: None,
                r#type: EntityType::ChildDevice,
                parent: None,
                payload: serde_json::json!({}),
            })
            .unwrap();

        let entity1 = store.get_by_id("test-device:device:child001:service:service001");
        assert_eq!(
            entity1.unwrap().entity_id,
            "test-device:device:child001:service:service001"
        );
    }
}
