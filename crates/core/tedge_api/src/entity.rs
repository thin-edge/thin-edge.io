use crate::control::DEFAULT_SERVICE_TYPE;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::TopicIdError;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

/// Represents externally provided unique ID of an entity.
///
/// Although this struct doesn't enforce any restrictions for the values,
/// the consumers may impose restrictions on the accepted values.

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityExternalId(String);

impl AsRef<str> for EntityExternalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// XXX: As `EntityExternalId` is used as a part of cloudbound MQTT topic, it
// can't contain characters invalid in topics, i.e. `+` and `#`. ([MQTT-4.7]).
// If it's derived from a MQTT topic, this holds, but if created from a string,
// this isn't checked, which is invalid!
impl From<&str> for EntityExternalId {
    fn from(val: &str) -> Self {
        Self(val.to_string())
    }
}

impl From<&String> for EntityExternalId {
    fn from(val: &String) -> Self {
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

impl From<&EntityExternalId> for String {
    fn from(value: &EntityExternalId) -> Self {
        value.0.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityMetadata {
    #[serde(rename = "@topic-id")]
    pub topic_id: EntityTopicId,
    #[serde(rename = "@parent", skip_serializing_if = "Option::is_none")]
    pub parent: Option<EntityTopicId>,
    #[serde(rename = "@type")]
    pub r#type: EntityType,
    #[serde(rename = "@id", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<EntityExternalId>,
    #[serde(rename = "@health", skip_serializing_if = "Option::is_none")]
    pub health_endpoint: Option<EntityTopicId>,

    #[serde(skip)]
    pub twin_data: Map<String, JsonValue>,

    /// Exposable configuration values collected from this entity's own retained
    /// `config` MQTT topic, keyed by configuration key. Each value keeps the type
    /// the setting has in `tedge.toml`, so a port stays a number and a flag stays
    /// a boolean.
    #[serde(skip)]
    pub config: BTreeMap<String, JsonValue>,
}

impl EntityMetadata {
    pub fn new(topic_id: EntityTopicId, r#type: EntityType) -> Self {
        Self {
            topic_id,
            r#type,
            external_id: None,
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
            config: BTreeMap::new(),
        }
    }

    pub fn with_external_id(mut self, external_id: EntityExternalId) -> Self {
        self.external_id = Some(external_id);
        self
    }

    pub fn with_parent(mut self, topic_id: EntityTopicId) -> Self {
        self.parent = Some(topic_id);
        self
    }

    pub fn with_health_endpoint(mut self, topic_id: EntityTopicId) -> Self {
        self.health_endpoint = Some(topic_id);
        self
    }

    /// Creates a entity metadata for the main device.
    pub fn main_device(device_id: Option<EntityExternalId>) -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: device_id,
            r#type: EntityType::MainDevice,
            parent: None,
            health_endpoint: None,
            twin_data: Map::new(),
            config: BTreeMap::new(),
        }
    }

    /// Creates a entity metadata for a child device.
    pub fn child_device(child_device_id: String) -> Result<Self, TopicIdError> {
        Ok(Self {
            topic_id: EntityTopicId::default_child_device(&child_device_id)?,
            external_id: Some(child_device_id.clone().into()),
            r#type: EntityType::ChildDevice,
            parent: Some(EntityTopicId::default_main_device()),
            health_endpoint: None,
            twin_data: Map::new(),
            config: BTreeMap::new(),
        })
    }

    /// Returns the default value of a twin key, used when the twin data has no value for it
    pub fn default_twin_value<'a>(
        &'a self,
        key: &str,
        config: &'a TwinDefaultsConfig,
    ) -> Option<&'a str> {
        match (self.r#type, key) {
            (EntityType::Service, "name") => self.topic_id.default_service_name(),
            (EntityType::Service, "type")
                if self
                    .parent
                    .as_ref()
                    .is_some_and(EntityTopicId::is_default_main_device) =>
            {
                Some(&config.service_type)
            }
            (EntityType::Service, "type") => Some(DEFAULT_SERVICE_TYPE),
            _ => None,
        }
    }

    /// Returns the twin value of a key, or its default value when the twin data has no value for it
    pub fn twin_value_with_default(
        &self,
        key: &str,
        config: &TwinDefaultsConfig,
    ) -> Option<JsonValue> {
        self.twin_data
            .get(key)
            .cloned()
            .or_else(|| self.default_twin_value(key, config).map(JsonValue::from))
    }

    /// Returns the twin data, with the default values of the keys it has no value for
    pub fn twin_data_with_defaults(&self, config: &TwinDefaultsConfig) -> Map<String, JsonValue> {
        let mut twin_data = Map::new();
        for key in ["name", "type"] {
            if let Some(value) = self.default_twin_value(key, config) {
                twin_data.insert(key.to_string(), value.into());
            }
        }
        twin_data.extend(self.twin_data.clone());
        twin_data
    }
}

/// The settings the default twin values depend on
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwinDefaultsConfig {
    /// The `service.type` setting
    service_type: String,
}

impl Default for TwinDefaultsConfig {
    fn default() -> Self {
        Self {
            service_type: DEFAULT_SERVICE_TYPE.to_string(),
        }
    }
}

impl TwinDefaultsConfig {
    /// Returns the settings for a `service.type` value, which is `service` when empty
    pub fn new(service_type: &str) -> Self {
        if service_type.is_empty() {
            Self::default()
        } else {
            Self {
                service_type: service_type.to_string(),
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    #[serde(rename = "device")]
    MainDevice,
    #[serde(rename = "child-device")]
    ChildDevice,
    #[serde(rename = "service")]
    Service,
}

impl EntityType {
    pub fn as_str(&self) -> &str {
        match self {
            EntityType::MainDevice => "device",
            EntityType::ChildDevice => "child-device",
            EntityType::Service => "service",
        }
    }
}

impl Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Error, PartialEq, Eq, Clone)]
#[error("Invalid entity type: {0}")]
pub struct InvalidEntityType(String);

impl FromStr for EntityType {
    type Err = InvalidEntityType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "device" => Ok(EntityType::MainDevice),
            "child-device" => Ok(EntityType::ChildDevice),
            "service" => Ok(EntityType::Service),
            other => Err(InvalidEntityType(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Unchanged,
    Updated,
    Inserted,
}
