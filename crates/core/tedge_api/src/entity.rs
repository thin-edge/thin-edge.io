use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::TopicIdError;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value as JsonValue;
use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

/// Represents externally provided unique ID of an entity.
///
/// Although this struct doesn't enforce any restrictions for the values,
/// the consumers may impose restrictions on the accepted values.

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
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

    #[serde(skip)]
    pub twin_data: Map<String, JsonValue>,
}

impl EntityMetadata {
    #[cfg(test)]
    pub(crate) fn new(
        topic_id: EntityTopicId,
        r#type: EntityType,
        parent: Option<EntityTopicId>,
        external_id: Option<EntityExternalId>,
    ) -> Self {
        Self {
            topic_id,
            external_id,
            r#type,
            parent,
            twin_data: Map::new(),
        }
    }

    /// Creates a entity metadata for the main device.
    pub fn main_device() -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: None,
            r#type: EntityType::MainDevice,
            parent: None,
            twin_data: Map::new(),
        }
    }

    /// Creates a entity metadata for a child device.
    pub fn child_device(child_device_id: String) -> Result<Self, TopicIdError> {
        Ok(Self {
            topic_id: EntityTopicId::default_child_device(&child_device_id)?,
            external_id: Some(child_device_id.clone().into()),
            r#type: EntityType::ChildDevice,
            parent: Some(EntityTopicId::default_main_device()),
            twin_data: Map::new(),
        })
    }

    pub fn display_name(&self) -> Option<&str> {
        self.twin_data.get("name").and_then(|v| v.as_str())
    }

    pub fn display_type(&self) -> Option<&str> {
        self.twin_data.get("type").and_then(|v| v.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
