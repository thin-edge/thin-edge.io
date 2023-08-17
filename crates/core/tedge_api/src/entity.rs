//! A module defining entities, their types, and utilities for parsing MQTT
//! topics following the default thin-edge MQTT scheme.

use std::str::FromStr;

// TODO: read from config
const MQTT_ROOT: &str = "te";

/// A thin-edge entity MQTT topic.
///
/// An entity topic consists of 3 groups: root, entity identifier, and
/// optionally a channel. To be a valid entity topic, a topic must start with a
/// root, and then have its entity identifier and channel (if present) groups
/// successfully parsed.
///
/// ```
/// # use tedge_api::entity::{EntityTopic, Channel, ChannelCategory};
/// let entity_topic: EntityTopic =
///     format!("te/device/child001/service/service001/m/measurement_type")
///         .parse()
///         .unwrap();
/// assert_eq!(entity_topic.entity_id(), "device/child001/service/service001");
/// assert_eq!(entity_topic.channel(), Some(&Channel {
///     category: ChannelCategory::Measurement,
///     r#type: "measurement_type".to_string(),
///     suffix: "".to_string()
/// }));
/// ```
///
/// https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#topic-scheme
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityTopic {
    entity_id: EntityId,
    channel: Option<Channel>,
}

impl EntityTopic {
    pub fn entity_id(&self) -> &str {
        self.entity_id.0.as_str()
    }

    pub fn channel(&self) -> Option<&Channel> {
        self.channel.as_ref()
    }

    /// Returns a device name if entity topic identifier is not using a custom
    /// schema.
    pub fn device_name(&self) -> Option<&str> {
        match self.entity_id.0.split('/').collect::<Vec<&str>>()[..] {
            ["device", device_id, "service", _] => Some(device_id),
            ["device", device_id, "", ""] => Some(device_id),
            _ => None,
        }
    }

    /// Returns a service name if entity topic identifier is not using a custom
    /// schema and the entity identifier refers to the service.
    pub fn service_name(&self) -> Option<&str> {
        match self.entity_id.0.split('/').collect::<Vec<&str>>()[..] {
            ["device", _, "service", service_id] => Some(service_id),
            _ => None,
        }
    }
}

impl FromStr for EntityTopic {
    type Err = EntityTopicError;

    fn from_str(topic: &str) -> Result<Self, Self::Err> {
        const ENTITY_ID_SEGMENTS: usize = 4;

        let (root, topic) = topic.split_once('/').ok_or(EntityTopicError::Root {
            expected: MQTT_ROOT.to_string(),
            got: topic.to_string(),
        })?;

        if root != MQTT_ROOT {
            return Err(EntityTopicError::Root {
                expected: MQTT_ROOT.to_string(),
                got: root.to_string(),
            });
        }

        let mut topic_separator_indices = topic.match_indices('/').map(|(i, _)| i);
        let id_channel_separator_index = topic_separator_indices.nth(3).unwrap_or(topic.len());

        let (entity_id, channel) = topic.split_at(id_channel_separator_index);

        let entity_id_segments = entity_id.matches('/').count();
        let missing_slashes = ENTITY_ID_SEGMENTS - entity_id_segments - 1;
        let entity_id = format!("{entity_id}{:/<1$}", "", missing_slashes);

        let channel = channel.trim_start_matches('/');
        let channel = if !channel.is_empty() {
            Some(Channel::new(channel)?)
        } else {
            None
        };

        Ok(EntityTopic {
            entity_id: EntityId(entity_id.to_string()),
            channel,
        })
    }
}

impl TryFrom<&mqtt_channel::Topic> for EntityTopic {
    type Error = EntityTopicError;

    fn try_from(topic: &mqtt_channel::Topic) -> Result<Self, Self::Error> {
        topic.name.parse()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum EntityTopicError {
    #[error("Fist topic segment expected to be {expected:?}, got {got:?}")]
    Root { expected: String, got: String },

    #[error("Channel group invalid")]
    Channel(#[from] ChannelError),
}

/// Represents an entity identifier group in thin-edge MQTT scheme.
///
/// An entity identifier is a fixed 4-segment group, as such any 4 topic
/// segments that come after the root are considered a part of an identifier,
/// even if they contain values usually present in the channel group, e.g.
/// `/m/`.
///
/// If the topic ends before the expected 4 segments, the remaining segments are
/// filled by empty segments (`//`).
///
/// # Example
///
///
/// https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-identifier
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityId(String);

/// Represents a channel group in thin-edge MQTT scheme.
///
/// A valid channel needs to be at least 2 segments long, with the first segment
/// containing a valid category.
///
/// <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-channel>
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub category: ChannelCategory,
    pub r#type: String,
    pub suffix: String,
}

impl Channel {
    pub fn new(channel: &str) -> Result<Self, ChannelError> {
        let (category, channel) = channel.split_once('/').ok_or(ChannelError::TooShort)?;
        let kind = match category {
            "m" => ChannelCategory::Measurement,
            "e" => ChannelCategory::Event,
            "a" => ChannelCategory::Alarm,
            "cmd" => ChannelCategory::Command,
            _ => return Err(ChannelError::InvalidCategory(category.to_string())),
        };

        let (r#type, suffix) = channel.split_once('/').unwrap_or((channel, ""));

        if r#type.is_empty() {
            return Err(ChannelError::TooShort);
        }

        Ok(Channel {
            category: kind,
            r#type: r#type.to_string(),
            suffix: suffix.to_string(),
        })
    }
}

impl FromStr for Channel {
    type Err = ChannelError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelCategory {
    Measurement,
    Event,
    Alarm,
    Command,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ChannelError {
    #[error("Channel needs to have at least 2 segments")]
    TooShort,

    #[error("Invalid category: {0:?}")]
    InvalidCategory(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_correct_topic() {
        let entity_topic: EntityTopic =
            format!("{MQTT_ROOT}/device/child001/service/service001/m/measurement_type")
                .parse()
                .unwrap();

        assert_eq!(
            entity_topic,
            EntityTopic {
                entity_id: EntityId("device/child001/service/service001".to_string()),
                channel: Some(Channel {
                    category: ChannelCategory::Measurement,
                    r#type: "measurement_type".to_string(),
                    suffix: "".to_string()
                })
            }
        );
    }

    #[test]
    fn parses_nochannel_correct_topic() {
        let topic1: EntityTopic = format!("{MQTT_ROOT}/device/child001/service/service001/")
            .parse()
            .unwrap();
        let topic2: EntityTopic = format!("{MQTT_ROOT}/device/child001/service/service001")
            .parse()
            .unwrap();

        let topic = EntityTopic {
            entity_id: EntityId("device/child001/service/service001".to_string()),
            channel: None,
        };

        assert_eq!(topic1, topic);
        assert_eq!(topic2, topic);
    }

    #[test]
    fn parses_noservice_entity_correct_topic() {
        let topic1: EntityTopic = format!("{MQTT_ROOT}/device/child001//").parse().unwrap();
        let topic2: EntityTopic = format!("{MQTT_ROOT}/device/child001").parse().unwrap();

        let topic = EntityTopic {
            entity_id: EntityId("device/child001//".to_string()),
            channel: None,
        };

        assert_eq!(topic1, topic);
        assert_eq!(topic2, topic);
    }

    #[test]
    fn no_root() {
        let topic = "device/child001/service/service001/m/measurement_type".parse::<EntityTopic>();

        assert!(topic.is_err());
    }

    #[test]
    fn incorrect_channel() {
        let topic1 = format!(
            "{MQTT_ROOT}/device/child001/service/service001/incorrect_category/measurement_type"
        )
        .parse::<EntityTopic>();

        let topic2 =
            format!("{MQTT_ROOT}/device/child001/service/service001/m/").parse::<EntityTopic>();

        let topic3 =
            format!("{MQTT_ROOT}/device/child001/service/service001/m").parse::<EntityTopic>();

        assert!(topic1.is_err());
        assert!(topic2.is_err());
        assert!(topic3.is_err());
    }
}
