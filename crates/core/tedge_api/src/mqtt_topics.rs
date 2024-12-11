//! This module abstracts the MQTT topics used by thin-edge.
//!
//! See https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/

use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::convert::Infallible;
use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;
use time::format_description;
use time::OffsetDateTime;

const ENTITY_ID_SEGMENTS: usize = 4;

/// The MQTT topics are represented by three distinct groups:
/// - a root prefix, used by all the topics
/// - an entity topic identifier of the source or target of the messages
/// - a channel kind for the messages exchanged along this topic
///
/// Once built from a root prefix, the main two features of such a schema are to:
/// - get the topic addressing a given entity channel
/// - get the entity channel addressed by some topic
///
/// ```
/// # use tedge_api::mqtt_topics::{MqttSchema, Channel, EntityTopicId};
/// # use mqtt_channel::Topic;
///
/// // The default root prefix is `"te"`:
/// let te = MqttSchema::default();
/// assert_eq!(&te.root, "te");
///
/// // Getting the entity channel addressed by some topic
/// let topic = Topic::new_unchecked("te/device/child001/service/service001/m/measurement_type");
/// let entity: EntityTopicId = "device/child001/service/service001".parse().unwrap();
/// let channel = Channel::Measurement {
///     measurement_type: "measurement_type".to_string(),
/// };
/// assert_eq!(
///     te.entity_channel_of(&topic).ok(),
///     Some((entity.clone(), channel.clone()))
/// );
///
/// // Getting the topic to address a specific entity channel
/// assert_eq!(
///     te.topic_for(&entity, &channel).name,
///     topic.name
/// );
/// ```
#[derive(Debug, Clone)]
pub struct MqttSchema {
    pub root: String,
}

/// The default schema using `te` for the root prefix
impl Default for MqttSchema {
    fn default() -> Self {
        MqttSchema::new()
    }
}

impl MqttSchema {
    /// Build a new schema using the default root prefix, i.e. `te`
    ///
    /// ```
    /// let te = tedge_api::mqtt_topics::MqttSchema::default();
    /// assert_eq!(&te.root, "te");
    /// ```
    pub fn new() -> Self {
        MqttSchema::with_root("te".to_string())
    }

    /// Build a new schema using the given root prefix for all topics.
    /// ```
    /// let te = tedge_api::mqtt_topics::MqttSchema::with_root("thin-edge".to_string());
    /// assert_eq!(&te.root, "thin-edge");
    /// ```
    pub fn with_root(root: String) -> Self {
        MqttSchema { root }
    }

    /// Build the schema to be used to decode a topic
    pub fn from_topic(topic: impl AsRef<str>) -> Self {
        let (root, _) = topic
            .as_ref()
            .split_once('/')
            .unwrap_or((topic.as_ref(), ""));
        Self::with_root(root.to_string())
    }

    /// Get the topic addressing a given entity channel
    /// ```
    /// # use tedge_api::mqtt_topics::{MqttSchema, Channel, EntityTopicId};
    /// # use mqtt_channel::Topic;
    ///
    /// let te = MqttSchema::default();
    /// let child_device: EntityTopicId = "device/child001//".parse().unwrap();
    /// let channel = Channel::AlarmMetadata {
    ///     alarm_type: "sensors".to_string(),
    /// };
    ///
    /// let topic = te.topic_for(&child_device, &channel);
    /// assert_eq!(
    ///     topic.name,
    ///     "te/device/child001///a/sensors/meta"
    /// );
    /// ```
    pub fn topic_for(&self, entity: &EntityTopicId, channel: &Channel) -> mqtt_channel::Topic {
        let channel = channel.to_string();
        let topic = if channel.is_empty() {
            format!("{}/{entity}", self.root)
        } else {
            format!("{}/{entity}/{channel}", self.root)
        };
        mqtt_channel::Topic::new(&topic).unwrap()
    }

    /// Get the entity channel addressed by some topic
    ///
    /// ```
    /// # use tedge_api::mqtt_topics::{MqttSchema, Channel, EntityTopicId};
    /// # use mqtt_channel::Topic;
    ///
    /// let te = MqttSchema::default();
    /// let topic = Topic::new_unchecked("te/device/child001/service/service001/m/measurement_type");
    ///
    /// let (entity_identifier, channel) = te.entity_channel_of(&topic).unwrap();
    /// assert_eq!(entity_identifier , "device/child001/service/service001");
    /// assert_eq!(channel, Channel::Measurement {
    ///     measurement_type: "measurement_type".to_string(),
    /// })
    /// ```
    pub fn entity_channel_of(
        &self,
        topic: impl AsRef<str>,
    ) -> Result<(EntityTopicId, Channel), EntityTopicError> {
        self.parse(topic.as_ref())
    }

    /// Get the topic filter to subscribe to messages from specific entities and channels
    ///
    /// ```
    /// use mqtt_channel::Topic;
    /// use tedge_api::mqtt_topics::{ChannelFilter, EntityFilter, MqttSchema};
    ///
    /// let te = MqttSchema::default();
    /// let topics = te.topics(EntityFilter::AnyEntity, ChannelFilter::Measurement);
    ///
    /// assert!(topics.accept_topic(&Topic::new_unchecked("te/device/main///m/")));
    /// assert!(topics.accept_topic(&Topic::new_unchecked("te/device/child///m/m_type")));
    /// assert!(topics.accept_topic(&Topic::new_unchecked("te/device/child/service/collected/m/collectd")));
    ///
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("not-te/device/main///m/")));
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("te/device/main///not-m/")));
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("te/device/main///m/t/not-meta")));
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("te/device/main///m/t/meta/too-long")));
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("te/device/main/too/short")));
    /// assert!(! topics.accept_topic(&Topic::new_unchecked("te/device/main/missing/sep/m")));
    /// ```
    pub fn topics(&self, entity: EntityFilter, channel: ChannelFilter) -> TopicFilter {
        let entity = match entity {
            EntityFilter::AnyEntity => "+/+/+/+".to_string(),
            EntityFilter::Entity(entity) => entity.to_string(),
        };
        let channel = match channel {
            ChannelFilter::EntityMetadata => "".to_string(),
            ChannelFilter::EntityTwinData => "/twin/+".to_string(),
            ChannelFilter::Measurement => "/m/+".to_string(),
            ChannelFilter::MeasurementMetadata => "/m/+/meta".to_string(),
            ChannelFilter::Event => "/e/+".to_string(),
            ChannelFilter::EventMetadata => "/e/+/meta".to_string(),
            ChannelFilter::Alarm => "/a/+".to_string(),
            ChannelFilter::AlarmMetadata => "/a/+/meta".to_string(),
            ChannelFilter::AnyCommand => "/cmd/+/+".to_string(),
            ChannelFilter::Command(operation) => format!("/cmd/{operation}/+"),
            ChannelFilter::AnyCommandMetadata => "/cmd/+".to_string(),
            ChannelFilter::CommandMetadata(operation) => format!("/cmd/{operation}"),
            ChannelFilter::Health => "/status/health".to_string(),
        };

        TopicFilter::new_unchecked(&format!("{}/{entity}{channel}", self.root))
    }

    /// Return the topic to publish an operation capability for a device
    pub fn capability_topic_for(&self, target: &EntityTopicId, operation: OperationType) -> Topic {
        self.topic_for(target, &Channel::CommandMetadata { operation })
    }

    /// Build a new error topic using the given schema for the root prefix.
    /// ```
    /// use mqtt_channel::Topic;
    /// let te = tedge_api::mqtt_topics::MqttSchema::with_root("thin-edge".to_string());
    /// assert_eq!(te.error_topic(), Topic::new_unchecked("thin-edge/errors"));
    /// ```
    pub fn error_topic(&self) -> Topic {
        Topic::new_unchecked(&format!("{0}/errors", self.root))
    }

    /// Extract the root prefix of a topic
    pub fn get_root_prefix(topic: impl AsRef<str>) -> Option<String> {
        match topic.as_ref().split('/').collect::<Vec<&str>>()[..] {
            [root, ..] => Some(root.to_string()),
            _ => None,
        }
    }

    /// Extract the entity identifier from a topic
    ///
    /// Note this function is not related to a specific topic root prefix
    pub fn get_entity_id(topic: impl AsRef<str>) -> Option<String> {
        match topic.as_ref().split('/').collect::<Vec<&str>>()[..] {
            [_, t1, t2, t3, t4, ..] => Some(format!("{t1}/{t2}/{t3}/{t4}")),
            _ => None,
        }
    }

    /// Extract the operation name from a command topic
    ///
    /// Note this function is not related to a specific topic root prefix
    pub fn get_operation_name(topic: impl AsRef<str>) -> Option<String> {
        match topic.as_ref().split('/').collect::<Vec<&str>>()[..] {
            [_, _, _, _, _, "cmd", op, ..] => Some(op.to_string()),
            _ => None,
        }
    }

    /// Extract the command instance identifier from a command topic
    ///
    /// Note this function is not related to a specific topic root prefix
    pub fn get_command_id(topic: impl AsRef<str>) -> Option<String> {
        match topic.as_ref().split('/').collect::<Vec<&str>>()[..] {
            [_, _, _, _, _, "cmd", _, id] => Some(id.to_string()),
            _ => None,
        }
    }

    fn parse(&self, topic: &str) -> Result<(EntityTopicId, Channel), EntityTopicError> {
        let (root, topic) = topic.split_once('/').ok_or(EntityTopicError::Root {
            expected: self.root.to_string(),
            got: topic.to_string(),
        })?;
        if root != self.root {
            return Err(EntityTopicError::Root {
                expected: self.root.to_string(),
                got: root.to_string(),
            });
        }

        let mut topic_separator_indices = topic.match_indices('/').map(|(i, _)| i);
        let id_channel_separator_index = topic_separator_indices.nth(3).unwrap_or(topic.len());

        let (entity_id, channel) = topic.split_at(id_channel_separator_index);
        let entity_id = entity_id.parse()?;
        let channel: Channel = channel.trim_start_matches('/').parse()?;
        Ok((entity_id, channel))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum EntityTopicError {
    #[error("Fist topic segment expected to be {expected:?}, got {got:?}")]
    Root { expected: String, got: String },

    #[error("Invalid entity topic identifier")]
    TopicId(#[from] TopicIdError),

    #[error("Channel group invalid")]
    Channel(#[from] ChannelError),
}

/// Represents an "Entity topic identifier" portion of the MQTT topic
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
/// - topic: `te/device/dev1/service/myservice/m//my_measurement`
/// - entity id: `device/dev1/service/myservice`
///
/// # Reference
/// https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub struct EntityTopicId(String);

impl<T: AsRef<str>> PartialEq<T> for EntityTopicId {
    fn eq(&self, other: &T) -> bool {
        self.0 == other.as_ref()
    }
}
impl PartialEq<str> for EntityTopicId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl Display for EntityTopicId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for EntityTopicId {
    type Err = TopicIdError;

    fn from_str(entity_id: &str) -> Result<Self, Self::Err> {
        let entity_id_segments = entity_id.matches('/').count() + 1;
        if entity_id_segments > ENTITY_ID_SEGMENTS {
            return Err(TopicIdError::TooLong);
        }

        let missing_slashes = ENTITY_ID_SEGMENTS - entity_id_segments;
        let topic_id = format!("{entity_id}{:/<1$}", "", missing_slashes);
        if mqtt_channel::Topic::new(&topic_id).is_err() {
            return Err(TopicIdError::InvalidMqttTopic);
        }

        Ok(EntityTopicId(topic_id))
    }
}

impl EntityTopicId {
    /// The default topic identifier for the main device.
    pub fn default_main_device() -> Self {
        EntityTopicId("device/main//".to_string())
    }

    /// The default topic identifier for a child device.
    pub fn default_child_device(child: &str) -> Result<Self, TopicIdError> {
        format!("device/{child}//").parse()
    }

    /// The default topic identifier for a service of the main device.
    pub fn default_main_service(service: &str) -> Result<Self, TopicIdError> {
        format!("device/main/service/{service}").parse()
    }

    /// The default topic identifier for a service of a child device.
    pub fn default_child_service(child: &str, service: &str) -> Result<Self, TopicIdError> {
        format!("device/{child}/service/{service}").parse()
    }

    /// Assuming `self` is a device in default MQTT scheme, create an
    /// `EntityTopicId` for a service on that device.
    ///
    /// Returns `None` if `self` is not in default MQTT scheme or if `service`
    /// is an invalid service name.
    pub fn default_service_for_device(&self, service: &str) -> Option<Self> {
        let device_name = self.default_device_name()?;
        Self::default_child_service(device_name, service).ok()
    }

    /// Returns true if the current topic id matches the default topic scheme:
    /// - device/<device-id>// : for devices
    /// - device/<device-id>/service/<service-id> : for services
    ///
    /// Returns false otherwise
    pub fn matches_default_topic_scheme(&self) -> bool {
        !default_topic_schema::parse(self).is_empty()
    }

    /// Returns the device name when the entity topic identifier is using the `device/+/service/+` pattern.
    ///
    /// Returns None otherwise.
    pub fn default_device_name(&self) -> Option<&str> {
        match self.0.split('/').collect::<Vec<&str>>()[..] {
            ["device", device_id, "service", _] => Some(device_id),
            ["device", device_id, "", ""] => Some(device_id),
            _ => None,
        }
    }

    /// Returns the service name when the entity topic identifier is using the `device/+/service/+` pattern.
    ///
    /// Returns None if this is not a service or if the pattern doesn't apply.
    pub fn default_service_name(&self) -> Option<&str> {
        match self.0.split('/').collect::<Vec<&str>>()[..] {
            ["device", _, "service", service_id] => Some(service_id),
            _ => None,
        }
    }

    /// Returns the topic identifier of the parent of a service,
    /// assuming `self` is the topic identifier of a service `device/+/service/+`
    ///
    /// Returns None if this is not a service or if the pattern doesn't apply.
    pub fn default_service_parent_identifier(&self) -> Option<Self> {
        match self.0.split('/').collect::<Vec<&str>>()[..] {
            ["device", parent_id, "service", _] => Some(parent_id),
            _ => None,
        }
        .map(|parent_id| EntityTopicId(format!("device/{parent_id}//")))
    }

    /// Returns true if the current topic identifier matches that of the main device
    pub fn is_default_main_device(&self) -> bool {
        self == &Self::default_main_device()
    }

    /// If `self` is a device topic id, return a service topic id under this
    /// device.
    ///
    /// The device topic id must be in a format: "device/DEVICE_NAME//"; if not,
    /// `None` will be returned.
    pub fn to_default_service_topic_id(&self, service_name: &str) -> Option<ServiceTopicId> {
        self.default_service_for_device(service_name)
            .map(ServiceTopicId)
    }

    /// Returns an array of all segments of this entity topic.
    fn segments(&self) -> [&str; ENTITY_ID_SEGMENTS] {
        let mut segments = self.0.split('/');
        let seg1 = segments.next().unwrap();
        let seg2 = segments.next().unwrap();
        let seg3 = segments.next().unwrap();
        let seg4 = segments.next().unwrap();
        [seg1, seg2, seg3, seg4]
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

pub mod default_topic_schema {
    use crate::entity_store::EntityRegistrationMessage;
    use crate::entity_store::EntityType;
    use crate::mqtt_topics::EntityTopicId;
    use serde_json::json;

    /// Parse an entity topic id into registration messages matching auto-registration logic.
    ///
    /// These registration messages are derived from the topic after the default topic schema
    /// - `device/main//` -> registration of the main device
    /// - `device/{child}//` -> registration of the child device
    /// - `device/{device}/service/{service}` -> registrations of the child device and of the service
    ///
    /// Return no registration messages if the entity topic id is not built after the default topic schema
    pub fn parse(topic_id: &EntityTopicId) -> Vec<EntityRegistrationMessage> {
        match topic_id.segments() {
            ["device", "main", "", ""] => vec![EntityRegistrationMessage {
                topic_id: topic_id.clone(),
                external_id: None,
                r#type: EntityType::MainDevice,
                parent: None,
                other: Default::default(),
            }],
            ["device", child, "", ""] if !child.is_empty() => vec![EntityRegistrationMessage {
                topic_id: topic_id.clone(),
                external_id: None,
                r#type: EntityType::ChildDevice,
                parent: Some(EntityTopicId::default_main_device()),
                other: json!({ "name": child }).as_object().unwrap().to_owned(),
            }],
            ["device", device, "service", service] if !device.is_empty() && !service.is_empty() => {
                // The device of a service has to be registered fist
                let device_topic_id = EntityTopicId::default_child_device(device).unwrap();
                let mut registrations = parse(&device_topic_id);

                // Then the service can be registered
                registrations.push(EntityRegistrationMessage {
                    topic_id: topic_id.clone(),
                    external_id: None,
                    r#type: EntityType::Service,
                    parent: Some(device_topic_id),
                    other: json!({ "name": service }).as_object().unwrap().to_owned(),
                });
                registrations
            }
            _ => vec![],
        }
    }
}

/// Contains a topic id of the service itself and the associated device.
pub struct Service {
    pub service_topic_id: ServiceTopicId,
    pub device_topic_id: DeviceTopicId,
}

/// Represents an entity topic identifier known to be a service.
///
/// It's most often in a format `device/DEVICE_NAME/service/SERVICE_NAME`, but
/// it doesn't have to be. Thus in order to know whether or not a particular
/// [`EntityTopicId`] is a service, one has to check the
/// [`EntityStore`](super::entity_store::EntityStore), but some functions do not
/// have any way to access it. As such, functions can use this type to tell the
/// caller that they expect passed [`EntityTopicId`] to be a service, and that
/// it is the responsibility of the caller to verify it first.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ServiceTopicId(EntityTopicId);

impl ServiceTopicId {
    pub fn new(entity_topic_id: EntityTopicId) -> Self {
        Self(entity_topic_id)
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn entity(&self) -> &EntityTopicId {
        &self.0
    }
}

impl From<EntityTopicId> for ServiceTopicId {
    fn from(value: EntityTopicId) -> Self {
        Self::new(value)
    }
}

impl Display for ServiceTopicId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Represents an entity topic identifier known to be a device.
///
/// It's most often in a format `device/DEVICE_NAME//`, but it doesn't have to
/// be. Thus in order to know whether or not a particular [`EntityTopicId`] is a
/// service, one has to check the
/// [`EntityStore`](super::entity_store::EntityStore), but some functions do not
/// have any way to access it. As such, functions can use this type to tell the
/// caller that they expect passed [`EntityTopicId`] to be a device, and that
/// it is the responsibility of the caller to verify it first.
pub struct DeviceTopicId(EntityTopicId);

impl DeviceTopicId {
    pub fn new(device_topic_id: EntityTopicId) -> Self {
        Self(device_topic_id)
    }

    pub fn entity(&self) -> &EntityTopicId {
        &self.0
    }
}

impl From<EntityTopicId> for DeviceTopicId {
    fn from(value: EntityTopicId) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum TopicIdError {
    #[error("An entity topic identifier has at most 4 segments")]
    TooLong,

    #[error("An entity topic identifier must be a valid MQTT topic")]
    InvalidMqttTopic,
}

/// A channel identifies the type of the messages exchanged over a topic
///
/// <https://thin-edge.github.io/thin-edge.io/next/references/mqtt-api/#group-channel>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Channel {
    EntityMetadata,
    EntityTwinData {
        fragment_key: String,
    },
    Measurement {
        measurement_type: String,
    },
    Event {
        event_type: String,
    },
    Alarm {
        alarm_type: String,
    },
    Command {
        operation: OperationType,
        cmd_id: String,
    },
    MeasurementMetadata {
        measurement_type: String,
    },
    EventMetadata {
        event_type: String,
    },
    AlarmMetadata {
        alarm_type: String,
    },
    CommandMetadata {
        operation: OperationType,
    },
    Health,
}

impl FromStr for Channel {
    type Err = ChannelError;

    fn from_str(channel: &str) -> Result<Self, ChannelError> {
        match channel.split('/').collect::<Vec<&str>>()[..] {
            [""] => Ok(Channel::EntityMetadata),
            ["twin", fragment_key] => Ok(Channel::EntityTwinData {
                fragment_key: fragment_key.to_string(),
            }),
            ["m", measurement_type] => Ok(Channel::Measurement {
                measurement_type: measurement_type.to_string(),
            }),
            ["m", measurement_type, "meta"] => Ok(Channel::MeasurementMetadata {
                measurement_type: measurement_type.to_string(),
            }),

            ["e", event_type] => Ok(Channel::Event {
                event_type: event_type.to_string(),
            }),
            ["e", event_type, "meta"] => Ok(Channel::EventMetadata {
                event_type: event_type.to_string(),
            }),

            ["a", alarm_type] => Ok(Channel::Alarm {
                alarm_type: alarm_type.to_string(),
            }),
            ["a", alarm_type, "meta"] => Ok(Channel::AlarmMetadata {
                alarm_type: alarm_type.to_string(),
            }),

            ["cmd", operation] => Ok(Channel::CommandMetadata {
                operation: operation.parse().unwrap(), // Infallible
            }),
            ["cmd", operation, cmd_id] => Ok(Channel::Command {
                operation: operation.parse().unwrap(), // Infallible
                cmd_id: cmd_id.to_string(),
            }),
            ["status", "health"] => Ok(Channel::Health),

            _ => Err(ChannelError::InvalidCategory(channel.to_string())),
        }
    }
}

impl Display for Channel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Channel::EntityMetadata => Ok(()),
            Channel::EntityTwinData { fragment_key } => write!(f, "twin/{fragment_key}"),

            Channel::Measurement { measurement_type } => write!(f, "m/{measurement_type}"),
            Channel::MeasurementMetadata { measurement_type } => {
                write!(f, "m/{measurement_type}/meta")
            }

            Channel::Event { event_type } => write!(f, "e/{event_type}"),
            Channel::EventMetadata { event_type } => write!(f, "e/{event_type}/meta"),

            Channel::Alarm { alarm_type } => write!(f, "a/{alarm_type}"),
            Channel::AlarmMetadata { alarm_type } => write!(f, "a/{alarm_type}/meta"),

            Channel::Command { operation, cmd_id } => write!(f, "cmd/{operation}/{cmd_id}"),
            Channel::CommandMetadata { operation } => write!(f, "cmd/{operation}"),
            Channel::Health => write!(f, "status/health"),
        }
    }
}

impl Channel {
    pub fn is_entity_metadata(&self) -> bool {
        matches!(self, Channel::EntityMetadata)
    }

    pub fn is_measurement(&self) -> bool {
        matches!(self, Channel::Measurement { .. })
    }

    pub fn is_health(&self) -> bool {
        matches!(self, Channel::Health)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OperationType {
    Restart,
    SoftwareList,
    SoftwareUpdate,
    LogUpload,
    ConfigSnapshot,
    ConfigUpdate,
    FirmwareUpdate,
    Health,
    DeviceProfile,
    Custom(String),
}

// Using a custom Serialize/Deserialize implementations to read "foo" as Custom("foo")
impl<'de> Deserialize<'de> for OperationType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        Ok(str.as_str().into())
    }
}

impl Serialize for OperationType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl FromStr for OperationType {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}

impl<'a> From<&'a str> for OperationType {
    fn from(s: &'a str) -> OperationType {
        match s {
            "restart" => OperationType::Restart,
            "software_list" => OperationType::SoftwareList,
            "software_update" => OperationType::SoftwareUpdate,
            "log_upload" => OperationType::LogUpload,
            "config_snapshot" => OperationType::ConfigSnapshot,
            "config_update" => OperationType::ConfigUpdate,
            "firmware_update" => OperationType::FirmwareUpdate,
            "device_profile" => OperationType::DeviceProfile,
            operation => OperationType::Custom(operation.to_string()),
        }
    }
}

impl From<&OperationType> for String {
    fn from(value: &OperationType) -> Self {
        format!("{value}")
    }
}

impl Display for OperationType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Restart => write!(f, "restart"),
            OperationType::SoftwareList => write!(f, "software_list"),
            OperationType::SoftwareUpdate => write!(f, "software_update"),
            OperationType::LogUpload => write!(f, "log_upload"),
            OperationType::ConfigSnapshot => write!(f, "config_snapshot"),
            OperationType::ConfigUpdate => write!(f, "config_update"),
            OperationType::FirmwareUpdate => write!(f, "firmware_update"),
            OperationType::Health => write!(f, "health"),
            OperationType::DeviceProfile => write!(f, "device_profile"),
            OperationType::Custom(operation) => write!(f, "{operation}"),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ChannelError {
    #[error("Channel needs to have at least 2 segments")]
    TooShort,

    #[error("Invalid category: {0:?}")]
    InvalidCategory(String),
}

pub enum EntityFilter<'a> {
    AnyEntity,
    Entity(&'a EntityTopicId),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChannelFilter {
    EntityMetadata,
    EntityTwinData,
    Measurement,
    Event,
    Alarm,
    AnyCommand,
    Command(OperationType),
    MeasurementMetadata,
    EventMetadata,
    AlarmMetadata,
    AnyCommandMetadata,
    CommandMetadata(OperationType),
    Health,
}

impl From<&Channel> for ChannelFilter {
    fn from(value: &Channel) -> Self {
        match value {
            Channel::EntityMetadata => ChannelFilter::EntityMetadata,
            Channel::EntityTwinData { fragment_key: _ } => ChannelFilter::EntityTwinData,
            Channel::Measurement {
                measurement_type: _,
            } => ChannelFilter::Measurement,
            Channel::Event { event_type: _ } => ChannelFilter::Event,
            Channel::Alarm { alarm_type: _ } => ChannelFilter::Alarm,
            Channel::Command {
                operation,
                cmd_id: _,
            } => ChannelFilter::Command(operation.clone()),
            Channel::MeasurementMetadata {
                measurement_type: _,
            } => ChannelFilter::MeasurementMetadata,
            Channel::EventMetadata { event_type: _ } => ChannelFilter::EventMetadata,
            Channel::AlarmMetadata { alarm_type: _ } => ChannelFilter::AlarmMetadata,
            Channel::CommandMetadata { operation } => {
                ChannelFilter::CommandMetadata(operation.clone())
            }
            Channel::Health => ChannelFilter::Health,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdGenerator {
    prefix: String,
}

impl IdGenerator {
    pub fn new(prefix: &str) -> Self {
        IdGenerator {
            prefix: prefix.into(),
        }
    }

    pub fn new_id(&self) -> String {
        format!(
            "{}-{}",
            self.prefix,
            OffsetDateTime::now_utc()
                .format(&format_description::well_known::Rfc3339)
                .unwrap(),
        )
    }

    pub fn new_id_with_str(&self, value: &str) -> String {
        format!("{}-{}", self.prefix, value)
    }

    pub fn is_generator_of(&self, cmd_id: &str) -> bool {
        cmd_id.contains(&self.prefix)
    }

    pub fn get_value<'a>(&self, cmd_id: &'a str) -> Option<&'a str> {
        cmd_id
            .strip_prefix(&self.prefix)
            .and_then(|s| s.strip_prefix('-'))
    }

    pub fn prefix(&self) -> &str {
        self.prefix.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    const MQTT_ROOT: &str = "test_te";

    #[test]
    fn parses_full_correct_topic() {
        let schema = MqttSchema::with_root(MQTT_ROOT.to_string());
        let entity_topic = schema
            .parse(&format!(
                "{MQTT_ROOT}/device/child001/service/service001/m/measurement_type"
            ))
            .unwrap();

        assert_eq!(
            entity_topic,
            (
                EntityTopicId("device/child001/service/service001".to_string()),
                Channel::Measurement {
                    measurement_type: "measurement_type".to_string(),
                }
            )
        );
    }

    #[test]
    fn parses_nochannel_correct_topic() {
        let schema = MqttSchema::with_root(MQTT_ROOT.to_string());
        let entity_channel = schema
            .parse(&format!("{MQTT_ROOT}/device/child001/service/service001"))
            .unwrap();

        let expected = (
            EntityTopicId("device/child001/service/service001".to_string()),
            Channel::EntityMetadata,
        );

        assert_eq!(entity_channel, expected);
    }

    #[test]
    fn parses_noservice_entity_correct_topic() {
        let schema = MqttSchema::with_root(MQTT_ROOT.to_string());
        let entity_channel1 = schema
            .parse(&format!("{MQTT_ROOT}/device/child001//"))
            .unwrap();
        let entity_channel2 = schema
            .parse(&format!("{MQTT_ROOT}/device/child001"))
            .unwrap();

        let expected = (
            EntityTopicId("device/child001//".to_string()),
            Channel::EntityMetadata,
        );

        assert_eq!(entity_channel1, expected);
        assert_eq!(entity_channel2, expected);
    }

    #[test]
    fn no_root() {
        let schema = MqttSchema::with_root(MQTT_ROOT.to_string());
        let entity_channel = schema.parse("device/child001/service/service001/m/measurement_type");

        assert!(entity_channel.is_err());
    }

    #[test]
    fn incorrect_channel() {
        let schema = MqttSchema::with_root(MQTT_ROOT.to_string());
        let entity_channel1 = schema.parse(&format!(
            "{MQTT_ROOT}/device/child001/service/service001/incorrect_category/measurement_type"
        ));

        let entity_channel2 =
            schema.parse(&format!("{MQTT_ROOT}/device/child001/service/service001/m"));

        assert!(entity_channel1.is_err());
        assert!(entity_channel2.is_err());
    }

    #[test_case("device/main//", true)]
    #[test_case("device/child//", true)]
    #[test_case("device///", false)]
    #[test_case("device/main/service/foo", true)]
    #[test_case("device/child/service/foo", true)]
    #[test_case("device/main//foo", false)]
    #[test_case("device/child/service/", false)]
    #[test_case("device//service/foo", false)]
    #[test_case("custom///", false)]
    #[test_case("custom/main//", false)]
    #[test_case("custom/child//", false)]
    #[test_case("custom/main/service/foo", false)]
    #[test_case("custom/child/service/foo", false)]
    #[test_case("device/main/custom_service/foo", false)]
    fn default_topic_scheme_match(topic: &str, matches: bool) {
        assert_eq!(
            EntityTopicId::from_str(topic)
                .unwrap()
                .matches_default_topic_scheme(),
            matches
        )
    }

    #[test]
    fn rejects_invalid_entity_topic_ids() {
        assert_eq!(
            "device/too/many/segments/".parse::<EntityTopicId>(),
            Err(TopicIdError::TooLong)
        );

        assert_eq!(
            "invalid/+/mqtttopic/#".parse::<EntityTopicId>(),
            Err(TopicIdError::InvalidMqttTopic)
        );
    }

    // TODO: we can forgot to update the test when adding variants, figure out a
    // way to use type system to fail if not all values checked
    #[test]
    fn topic_for() {
        let mqtt_schema = MqttSchema::new();

        let device: EntityTopicId = "device/main//".parse().unwrap();

        assert_eq!(
            mqtt_schema.topic_for(&device, &Channel::EntityMetadata),
            mqtt_channel::Topic::new_unchecked("te/device/main//")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::Measurement {
                    measurement_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///m/type")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::MeasurementMetadata {
                    measurement_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///m/type/meta")
        );

        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::Event {
                    event_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///e/type")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::EventMetadata {
                    event_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///e/type/meta")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::Alarm {
                    alarm_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///a/type")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::AlarmMetadata {
                    alarm_type: "type".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///a/type/meta")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::Command {
                    operation: OperationType::Health,
                    cmd_id: "check".to_string()
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///cmd/health/check")
        );
        assert_eq!(
            mqtt_schema.topic_for(
                &device,
                &Channel::CommandMetadata {
                    operation: OperationType::LogUpload
                }
            ),
            mqtt_channel::Topic::new_unchecked("te/device/main///cmd/log_upload")
        );
        assert_eq!(
            mqtt_schema.topic_for(&device, &Channel::Health),
            mqtt_channel::Topic::new_unchecked("te/device/main///status/health")
        );
    }

    #[test_case("abc-1234", Some("1234"))]
    #[test_case("abc-", Some(""))]
    #[test_case("abc", None)]
    #[test_case("1234", None)]
    fn extract_value_from_cmd_id(cmd_id: &str, expected: Option<&str>) {
        let id_generator = IdGenerator::new("abc");
        let maybe_id = id_generator.get_value(cmd_id);
        assert_eq!(maybe_id, expected);
    }
}
