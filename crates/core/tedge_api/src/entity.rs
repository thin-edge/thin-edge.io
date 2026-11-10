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

    /// Describe the registration fields that differ from a previous version of this entity
    ///
    /// Returns a comma-separated list of `key: old -> new` pairs, using the keys of a registration message
    /// (e.g. `@parent: "device/main//" -> "device/child1//", name: "foo" -> "bar"`),
    /// or an empty string if nothing changed.
    /// A field missing from one of the versions is shown as `null`.
    pub fn changes_from(&self, previous: &EntityMetadata) -> String {
        let previous = previous.registration_fields();
        let current = self.registration_fields();
        let keys: std::collections::BTreeSet<&String> =
            previous.keys().chain(current.keys()).collect();
        keys.into_iter()
            .filter_map(|key| {
                let old = previous.get(key).unwrap_or(&JsonValue::Null);
                let new = current.get(key).unwrap_or(&JsonValue::Null);
                (old != new).then(|| format!("{key}: {old} -> {new}"))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The fields of this entity as they appear in a registration message
    fn registration_fields(&self) -> Map<String, JsonValue> {
        let mut fields = match serde_json::to_value(self) {
            Ok(JsonValue::Object(fields)) => fields,
            _ => Map::new(),
        };
        fields.extend(self.twin_data.clone());
        fields
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

    pub fn display_name(&self) -> Option<&str> {
        self.twin_data
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| match self.r#type {
                EntityType::MainDevice => self.external_id.as_ref().map(|xid| xid.as_ref()),
                EntityType::ChildDevice => self.topic_id.default_device_name(),
                EntityType::Service => self.topic_id.default_service_name(),
            })
    }

    pub fn display_type(&self) -> Option<&str> {
        self.twin_data.get("type").and_then(|v| v.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn changes_from_lists_only_the_modified_fields() {
        let child = EntityTopicId::default_child_device("child1").unwrap();
        let mut previous = EntityMetadata::new(child.clone(), EntityType::ChildDevice)
            .with_parent(EntityTopicId::default_main_device());
        previous.twin_data.insert("name".into(), json!("foo"));
        previous.twin_data.insert("type".into(), json!("sensor"));

        let mut current = previous
            .clone()
            .with_parent(EntityTopicId::default_child_device("child0").unwrap())
            .with_health_endpoint(child);
        current.twin_data.insert("name".into(), json!("bar"));

        assert_eq!(
            current.changes_from(&previous),
            r#"@health: null -> "device/child1//", @parent: "device/main//" -> "device/child0//", name: "foo" -> "bar""#
        );
        assert_eq!(current.changes_from(&current), "");
    }
}
