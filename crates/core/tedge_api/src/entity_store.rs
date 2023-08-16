//! A store containing registered MQTT entities.
//!
//! References:
//!
//! - <https://github.com/thin-edge/thin-edge.io/issues/2081>
//! - <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#entity-store>

// TODO: move entity business logic to its own module

use std::collections::HashMap;

use mqtt_channel::Message;
use mqtt_channel::Topic;

/// Represents an "MQTT entity identifier" portion of the MQTT topic
///
/// Example:
/// - topic: `te/device/dev1/service/myservice/m//my_measurement
/// - entity id: `device/dev1/service/myservice`
///
/// <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-identifier>
type EntityId = String;

// type checker is not able to infer `&EntityId` as &str, so alias is needed
//
// TODO: try using Rc, Cow, or string interning to get rid of duplicate strings
type EntityIdRef<'a> = &'a str;

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
/// let registration_message = EntityRegistrationMessage::try_from(mqtt_message).unwrap();
///
/// let mut entity_store = EntityStore::with_main_device(registration_message);
/// ```
#[derive(Debug, Clone)]
pub struct EntityStore {
    main_device: EntityId,
    entities: HashMap<EntityId, EntityMetadata>,
    external_id_index: HashMap<String, EntityId>,
}

impl EntityStore {
    /// Creates a new entity store with a given main device.
    pub fn with_main_device(main_device: EntityRegistrationMessage) -> Option<Self> {
        if main_device.r#type != EntityType::MainDevice {
            return None;
        }

        let external_id = main_device.external_id?;
        let metadata = EntityMetadata {
            external_id: external_id.clone(),
            r#type: main_device.r#type,
            parent: None,
            other: main_device.payload,
        };

        Some(EntityStore {
            main_device: main_device.mqtt_id.clone(),
            entities: HashMap::from([(main_device.mqtt_id.clone(), metadata)]),
            external_id_index: HashMap::from([(external_id, main_device.mqtt_id)]),
        })
    }

    /// Returns information about an entity under a given MQTT entity topic.
    pub fn get(&self, entity_id: &str) -> Option<&EntityMetadata> {
        self.entities.get(entity_id)
    }

    /// Returns information for an entity under a given external id.
    pub fn get_by_external_id(&self, external_id: &str) -> Option<&EntityMetadata> {
        let mqtt_id = self.external_id_index.get(external_id)?;
        self.get(mqtt_id)
    }

    /// Returns the MQTT identifier of the main device.
    ///
    /// The main device is an entity with `@type: "device"`.
    pub fn main_device(&self) -> EntityIdRef {
        self.main_device.as_str()
    }

    /// Returns MQTT identifiers of child devices of a given device.
    pub fn child_devices(&self, entity_topic: EntityIdRef) -> Vec<EntityIdRef> {
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
    pub fn services(&self, entity_topic: EntityIdRef) -> Vec<EntityIdRef> {
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
    pub fn update(&mut self, message: EntityRegistrationMessage) -> Result<Vec<EntityId>, Error> {
        if message.r#type == EntityType::MainDevice && message.mqtt_id != self.main_device {
            return Err(Error::MainDeviceAlreadyRegistered(
                self.main_device.as_str().into(),
            ));
        }

        let mut affected_entities = vec![];

        let parent = match message.r#type {
            EntityType::ChildDevice => message.parent.or(Some(self.main_device.clone())),
            EntityType::Service => message.parent.or(Some(self.main_device.clone())),
            EntityType::MainDevice => None,
        };

        // parent device is affected if new device is its child
        if let Some(parent) = &parent {
            if !self.entities.contains_key(parent) {
                return Err(Error::NoParent(parent.clone().into_boxed_str()));
            }

            affected_entities.push(parent.clone());
        }

        let external_id = message
            .external_id
            .unwrap_or_else(|| self.derive_external_id(&message.mqtt_id));
        let entity_metadata = EntityMetadata {
            r#type: message.r#type,
            external_id: external_id.clone(),
            parent,
            other: message.payload,
        };

        // device is affected if it was previously registered and was updated
        let previous = self
            .entities
            .insert(message.mqtt_id.clone(), entity_metadata);

        if previous.is_some() {
            affected_entities.push(message.mqtt_id);
        } else {
            self.external_id_index.insert(external_id, message.mqtt_id);
        }

        Ok(affected_entities)
    }

    /// An iterator over all registered entities.
    pub fn iter(&self) -> impl Iterator<Item = (&EntityId, &EntityMetadata)> {
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
    fn derive_external_id(&self, entity_topic: EntityIdRef) -> String {
        if entity_topic == self.main_device {
            self.get(&self.main_device).unwrap().external_id.to_string()
        } else {
            let main_device_external_id = &self.get(&self.main_device).unwrap().external_id;
            let external_id_suffix = entity_topic.replace('/', ":");
            let external_id_suffix = external_id_suffix.trim_matches(':');

            format!("{main_device_external_id}:{external_id_suffix}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    parent: Option<EntityId>,
    r#type: EntityType,
    external_id: String,
    other: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntityType {
    MainDevice,
    ChildDevice,
    Service,
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
    mqtt_id: EntityId,
    external_id: Option<String>,
    r#type: EntityType,
    parent: Option<EntityId>,
    payload: serde_json::Value,
}

impl EntityRegistrationMessage {
    /// Parses a MQTT message as an entity registration message.
    ///
    /// MQTT message is an entity registration message if
    /// - published on a prefix of `te/+/+/+/+`
    /// - its payload contains a registration message.
    pub fn new(message: Message) -> Option<Self> {
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

        let external_id = payload
            .get("@id")
            .and_then(|id| id.as_str())
            .map(|id| id.to_string());

        let mqtt_id = message
            .topic
            .name
            .strip_prefix(MQTT_ROOT)
            .and_then(|s| s.strip_prefix('/'))?;

        Some(Self {
            mqtt_id: mqtt_id.to_string(),
            external_id,
            r#type,
            parent,
            payload,
        })
    }

    /// Creates a entity registration message for a main device.
    pub fn main_device(external_id: String) -> Self {
        Self {
            mqtt_id: "device/main//".to_string(),
            external_id: Some(external_id),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: serde_json::json!({}),
        }
    }
}

impl TryFrom<Message> for EntityRegistrationMessage {
    type Error = ();

    fn try_from(value: Message) -> Result<Self, Self::Error> {
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
/// # use tedge_api::entity_store::entity_mqtt_id;
/// let entity_measurement_topic = Topic::new("te/device/main/service/my_service/m/my_measurement").unwrap();
/// assert_eq!(entity_mqtt_id(&entity_measurement_topic), Some("device/main/service/my_service"));

/// let custom_topic = Topic::new("te/device/1/2/3/m/my_measurement").unwrap();
/// assert_eq!(entity_mqtt_id(&custom_topic), Some("device/1/2/3"));
///
/// let custom_topic = Topic::new("custom_root/device/1/2/3/m/my_measurement").unwrap();
/// assert_eq!(entity_mqtt_id(&custom_topic), None);
/// ```
// TODO: this should be moved to MQTT parsing module when it's created
// https://github.com/thin-edge/thin-edge.io/pull/2118#issuecomment-1668110422
pub fn entity_mqtt_id(topic: &Topic) -> Option<&str> {
    let topic = topic
        .name
        .strip_prefix(MQTT_ROOT)
        .and_then(|s| s.strip_prefix('/'))?;

    let identifier_len = topic
        .match_indices('/')
        .nth(3)
        .map(|(i, _)| i)
        .unwrap_or(topic.len());

    Some(&topic[..identifier_len])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn registers_main_device() {
        let store = EntityStore::with_main_device(EntityRegistrationMessage {
            mqtt_id: "device/main//".to_string(),
            external_id: Some("test-device".to_string()),
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
            mqtt_id: "device/main//".to_string(),
            external_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({"@type": "device"}),
        })
        .unwrap();

        // If the @parent info is not provided, it is assumed to be an immediate
        // child of the main device.
        let updated_entities = store
            .update(
                EntityRegistrationMessage::new(Message::new(
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
                EntityRegistrationMessage::new(Message::new(
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
            external_id: Some("test-device".to_string()),
            mqtt_id: "device/main//".to_string(),
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        // Services are namespaced under devices, so `parent` is not necessary
        let updated_entities = store
            .update(EntityRegistrationMessage {
                r#type: EntityType::Service,
                external_id: None,
                mqtt_id: "device/main/service/service1".to_string(),
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
                external_id: None,
                mqtt_id: "device/main/service/service2".to_string(),
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
            mqtt_id: "device/main//".try_into().unwrap(),
            r#type: EntityType::MainDevice,
            external_id: Some("test-device".to_string()),
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        let res = store.update(EntityRegistrationMessage {
            mqtt_id: "device/another_main//".try_into().unwrap(),
            external_id: Some("test-device".to_string()),
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
            mqtt_id: "device/main//".try_into().unwrap(),
            external_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        let res = store.update(EntityRegistrationMessage {
            mqtt_id: "device/main//".try_into().unwrap(),
            external_id: None,
            r#type: EntityType::ChildDevice,
            parent: Some("device/myawesomeparent//".to_string()),
            payload: json!({}),
        });

        assert!(matches!(res, Err(Error::NoParent(_))));
    }

    #[test]
    fn generates_external_ids() {
        let mut store = EntityStore::with_main_device(EntityRegistrationMessage {
            mqtt_id: "device/main//".try_into().unwrap(),
            external_id: Some("test-device".to_string()),
            r#type: EntityType::MainDevice,
            parent: None,
            payload: json!({}),
        })
        .unwrap();

        store
            .update(EntityRegistrationMessage {
                mqtt_id: "device/child001/service/service001".to_string(),
                external_id: None,
                r#type: EntityType::ChildDevice,
                parent: None,
                payload: serde_json::json!({}),
            })
            .unwrap();

        let entity1 = store.get_by_external_id("test-device:device:child001:service:service001");
        assert_eq!(
            entity1.unwrap().external_id,
            "test-device:device:child001:service:service001"
        );
    }
}
