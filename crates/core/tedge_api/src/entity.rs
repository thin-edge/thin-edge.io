use crate::entity_store::EntityType;
use crate::mqtt_topics::EntityTopicId;
use crate::mqtt_topics::TopicIdError;
use serde::Deserialize;
use serde::Serialize;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityMetadata {
    pub topic_id: EntityTopicId,
    pub external_id: EntityExternalId,
    pub r#type: EntityType,
    pub parent: Option<EntityTopicId>,
    pub display_name: Option<String>,
    pub display_type: Option<String>,
}

impl EntityMetadata {
    /// Creates a entity metadata for the main device.
    pub fn main_device(device_id: String) -> Self {
        Self {
            topic_id: EntityTopicId::default_main_device(),
            external_id: device_id.clone().into(),
            r#type: EntityType::MainDevice,
            parent: None,
            display_name: Some(device_id),
            display_type: None,
        }
    }

    /// Creates a entity metadata for a child device.
    pub fn child_device(child_device_id: String) -> Result<Self, TopicIdError> {
        Ok(Self {
            topic_id: EntityTopicId::default_child_device(&child_device_id)?,
            external_id: child_device_id.clone().into(),
            r#type: EntityType::ChildDevice,
            parent: Some(EntityTopicId::default_main_device()),
            display_name: Some(child_device_id),
            display_type: None,
        })
    }
}
