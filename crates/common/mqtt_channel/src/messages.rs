use crate::errors::MqttError;
use crate::topics::Topic;
use rumqttc::Publish;
use rumqttc::QoS;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::fmt::Write;

/// A message to be sent to or received from MQTT.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MqttMessage {
    pub topic: Topic,
    pub payload: DebugPayload,
    #[serde(serialize_with = "serialize_qos", deserialize_with = "deserialize_qos")]
    pub qos: QoS,
    pub retain: bool,
}

impl Display for MqttMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_char('[')?;
        f.write_str(&self.topic.name)?;
        f.write_str(" qos=")?;
        f.write_char(match self.qos {
            QoS::AtMostOnce => '0',
            QoS::AtLeastOnce => '1',
            QoS::ExactlyOnce => '2',
        })?;
        f.write_str(if self.retain { " retained] " } else { "] " })?;
        Display::fmt(&self.payload, f)
    }
}

pub fn serialize_qos<S>(qos: &QoS, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    (*qos as u8).serialize(serializer)
}

pub fn deserialize_qos<'de, D>(deserializer: D) -> Result<QoS, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = u8::deserialize(deserializer)?;
    match value {
        0 => Ok(QoS::AtMostOnce),
        1 => Ok(QoS::AtLeastOnce),
        2 => Ok(QoS::ExactlyOnce),
        _ => Err(serde::de::Error::custom("Invalid QoS value")),
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct DebugPayload(Payload);

impl Debug for DebugPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Ok(str) => {
                f.write_char('"')?;
                f.write_str(str)?;
                f.write_char('"')
            }
            Err(_) => self.0.fmt(f),
        }
    }
}

impl From<String> for DebugPayload {
    fn from(value: String) -> Self {
        DebugPayload::new(value)
    }
}

impl From<DebugPayload> for Vec<u8> {
    fn from(value: DebugPayload) -> Self {
        value.0
    }
}

impl From<Vec<u8>> for DebugPayload {
    fn from(value: Vec<u8>) -> Self {
        DebugPayload::new(value)
    }
}

impl AsRef<Payload> for DebugPayload {
    fn as_ref(&self) -> &Payload {
        &self.0
    }
}

impl Serialize for DebugPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match std::str::from_utf8(&self.0) {
            Ok(payload_str) => {
                // Serialize as a string if all characters are valid UTF-8
                serializer.serialize_str(payload_str)
            }
            Err(_) => {
                // Serialize as a byte array otherwise
                serializer.serialize_bytes(&self.0)
            }
        }
    }
}

impl<'de> Deserialize<'de> for DebugPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DebugPayloadVisitor;

        impl serde::de::Visitor<'_> for DebugPayloadVisitor {
            type Value = DebugPayload;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or a sequence of bytes")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(DebugPayload::new(value))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(DebugPayload::new(value))
            }
        }

        deserializer.deserialize_any(DebugPayloadVisitor)
    }
}

impl Display for DebugPayload {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.as_str() {
            Ok(str) => f.write_str(str),
            Err(_) => f.write_str(&format!("non UTF-8 payload of {} bytes", self.0.len())),
        }
    }
}

impl DebugPayload {
    /// Remove any trailing null char
    fn new(payload: impl Into<Vec<u8>>) -> Self {
        let mut payload = payload.into();
        if payload.ends_with(b"\0") {
            payload.pop();
        };
        DebugPayload(payload)
    }

    /// The payload string (unless this payload is not UTF8)
    pub fn as_str(&self) -> Result<&str, MqttError> {
        let bytes = self.as_bytes();
        std::str::from_utf8(bytes).map_err(|err| MqttError::new_invalid_utf8_payload(bytes, err))
    }

    /// The bytes of the payload
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// A message payload
pub type Payload = Vec<u8>;

/// The number of bytes the MQTT "remaining length" field occupies for a packet
/// whose remaining length is `len`
fn remaining_length_size(len: usize) -> usize {
    if len >= 2_097_152 {
        4
    } else if len >= 16_384 {
        3
    } else if len >= 128 {
        2
    } else {
        1
    }
}

/// Computes the wire size, in bytes, of an MQTT PUBLISH packet with the given
/// topic, QoS and payload length
///
/// Matches `rumqttc::Publish::size()`
pub fn publish_packet_size(topic: &str, qos: QoS, payload_len: usize) -> usize {
    const TOPIC_LENGTH_SIZE: usize = 2; // 2 bytes for the topic length prefix
    const FIXED_HEADER_SIZE: usize = 1; // 1 byte for the fixed header
    let packet_id_size = if qos == QoS::AtMostOnce { 0 } else { 2 };
    let remaining_length = TOPIC_LENGTH_SIZE + topic.len() + packet_id_size + payload_len;
    FIXED_HEADER_SIZE + remaining_length_size(remaining_length) + remaining_length
}

impl MqttMessage {
    pub fn new<B>(topic: &Topic, payload: B) -> MqttMessage
    where
        B: Into<Payload>,
    {
        MqttMessage {
            topic: topic.clone(),
            payload: DebugPayload::new(payload),
            qos: QoS::AtLeastOnce,
            retain: false,
        }
    }

    pub fn with_qos(self, qos: QoS) -> Self {
        Self { qos, ..self }
    }

    pub fn with_retain(self) -> Self {
        Self {
            retain: true,
            ..self
        }
    }

    pub fn with_retain_flag(self, retain: bool) -> Self {
        Self { retain, ..self }
    }

    /// The message payload
    pub fn payload(&self) -> &Payload {
        &self.payload.0
    }

    /// The payload string (unless this payload is not UTF8)
    pub fn payload_str(&self) -> Result<&str, MqttError> {
        self.payload.as_str()
    }

    /// The bytes of the payload (except any trailing null char)
    pub fn payload_bytes(&self) -> &[u8] {
        self.payload.as_bytes()
    }

    /// Split the message into a (topic, payload) pair
    pub fn split(self) -> (String, Payload) {
        (self.topic.name, self.payload.0)
    }

    /// The size, in bytes, this message occupies as an MQTT PUBLISH packet on the wire
    pub fn wire_size(&self) -> usize {
        publish_packet_size(&self.topic.name, self.qos, self.payload().len())
    }
}

impl From<MqttMessage> for Publish {
    fn from(val: MqttMessage) -> Self {
        let mut publish = Publish::new(&val.topic.name, val.qos, val.payload.0);
        publish.retain = val.retain;
        publish
    }
}

impl From<Publish> for MqttMessage {
    fn from(msg: Publish) -> Self {
        let Publish {
            topic,
            payload,
            qos,
            retain,
            ..
        } = msg;

        MqttMessage {
            topic: Topic::new_unchecked(&topic),
            payload: DebugPayload::new(payload),
            qos,
            retain,
        }
    }
}

impl<T, U> From<(T, U)> for MqttMessage
where
    T: AsRef<str>,
    U: AsRef<str>,
{
    fn from(value: (T, U)) -> Self {
        MqttMessage::new(&Topic::new_unchecked(value.0.as_ref()), value.1.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn check_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b"123\0"[..]);

        assert_eq!(message.payload_bytes(), b"123");
    }

    #[test]
    fn payload_bytes_removes_only_last_null_char() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b"123\0\0"[..]);

        assert_eq!(message.payload_bytes(), b"123\0");
    }

    #[test]
    fn check_empty_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b""[..]);

        assert_eq!(message.payload_bytes(), b"");
    }
    #[test]
    fn check_non_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b"123"[..]);

        assert_eq!(message.payload_bytes(), b"123");
    }
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_middle() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b"temperature\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 11: temperature..."
        );
    }
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_beginning() {
        let topic = Topic::new("trimmed").unwrap();
        let message = MqttMessage::new(&topic, &b"\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 0: ..."
        );
    }

    #[test]
    fn wire_size_accounts_for_topic_and_framing() {
        // topic "a/b" (3) + 2-byte length prefix + 2-byte packet id (QoS 1)
        // + payload "xy" (2) => remaining length 9, encoded in 1 byte,
        // plus the 1-byte control byte => 11 bytes on the wire.
        let message = MqttMessage::new(&Topic::new_unchecked("a/b"), "xy");
        assert_eq!(message.wire_size(), 11);
    }

    #[test]
    fn wire_size_omits_packet_id_at_qos_0() {
        let topic = Topic::new_unchecked("a/b");
        let qos_1 = MqttMessage::new(&topic, "xy").with_qos(QoS::AtLeastOnce);
        let qos_0 = MqttMessage::new(&topic, "xy").with_qos(QoS::AtMostOnce);

        // The QoS-0 packet carries no packet identifier, so it is 2 bytes smaller.
        assert_eq!(qos_1.wire_size(), 11);
        assert_eq!(qos_0.wire_size(), 9);
    }

    #[test]
    fn payload_within_limit_but_packet_over_limit() {
        let limit = 20;
        let payload = vec![b'x'; 18];
        // Body of 18 is within the limit of 20...
        assert!(payload.len() <= limit);
        let message = MqttMessage::new(&Topic::new_unchecked("topic"), payload);
        // ...but the full packet (topic + framing + payload) exceeds it.
        assert!(message.wire_size() > limit);
    }

    #[test]
    fn wire_size_matches_rumqttc_publish_size() {
        let topic = "some/longer/topic/name";
        let payload = vec![b'p'; 200];

        // QoS 0: rumqttc never adds a packet id.
        let qos_0 = Publish::new(topic, QoS::AtMostOnce, payload.clone());
        assert_eq!(
            publish_packet_size(topic, QoS::AtMostOnce, payload.len()),
            qos_0.size()
        );

        // QoS 1: rumqttc only counts the packet id once one has been assigned.
        let mut qos_1 = Publish::new(topic, QoS::AtLeastOnce, payload.clone());
        qos_1.pkid = 1;
        assert_eq!(
            publish_packet_size(topic, QoS::AtLeastOnce, payload.len()),
            qos_1.size()
        );
    }

    #[test]
    fn message_serialize_deserialize() {
        let message = MqttMessage {
            topic: Topic::new("test").unwrap(),
            payload: DebugPayload::new("test-payload"),
            qos: QoS::AtMostOnce,
            retain: true,
        };

        let json = serde_json::to_value(&message).expect("Serialization failed");
        assert_eq!(json.get("payload").unwrap(), &json!("test-payload"));
        let deserialized: MqttMessage =
            serde_json::from_value(json).expect("Deserialization failed");
        assert_eq!(deserialized, message);
    }
}
