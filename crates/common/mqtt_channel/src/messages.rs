use crate::errors::MqttError;
use crate::topics::Topic;
use rumqttc::Publish;
use rumqttc::QoS;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fmt::Write;

/// A message to be sent to or received from MQTT.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Message {
    pub topic: Topic,
    pub payload: DebugPayload,
    pub qos: QoS,
    pub retain: bool,
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
        DebugPayload(value.into_bytes())
    }
}

impl AsRef<Payload> for DebugPayload {
    fn as_ref(&self) -> &Payload {
        &self.0
    }
}

impl DebugPayload {
    /// The payload string (unless this payload is not UTF8)
    pub fn as_str(&self) -> Result<&str, MqttError> {
        let bytes = self.as_bytes();
        std::str::from_utf8(bytes).map_err(|err| MqttError::new_invalid_utf8_payload(bytes, err))
    }

    /// The bytes of the payload (except any trailing null char)
    pub fn as_bytes(&self) -> &[u8] {
        self.0.strip_suffix(&[0]).unwrap_or(self.0.as_slice())
    }
}

/// A message payload
pub type Payload = Vec<u8>;

impl Message {
    pub fn new<B>(topic: &Topic, payload: B) -> Message
    where
        B: Into<Payload>,
    {
        Message {
            topic: topic.clone(),
            payload: DebugPayload(payload.into()),
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
}

impl From<Message> for Publish {
    fn from(val: Message) -> Self {
        let mut publish = Publish::new(&val.topic.name, val.qos, val.payload.0);
        publish.retain = val.retain;
        publish
    }
}

impl From<Publish> for Message {
    fn from(msg: Publish) -> Self {
        let Publish {
            topic,
            payload,
            qos,
            retain,
            ..
        } = msg;

        Message {
            topic: Topic::new_unchecked(&topic),
            payload: DebugPayload(payload.to_vec()),
            qos,
            retain,
        }
    }
}

impl<T, U> From<(T, U)> for Message
where
    T: AsRef<str>,
    U: AsRef<str>,
{
    fn from(value: (T, U)) -> Self {
        Message::new(&Topic::new_unchecked(value.0.as_ref()), value.1.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123\0"[..]);

        assert_eq!(message.payload_bytes(), b"123");
    }

    #[test]
    fn payload_bytes_removes_only_last_null_char() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123\0\0"[..]);

        assert_eq!(message.payload_bytes(), b"123\0");
    }

    #[test]
    fn check_empty_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b""[..]);

        assert_eq!(message.payload_bytes(), b"");
    }
    #[test]
    fn check_non_null_terminated_messages() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"123"[..]);

        assert_eq!(message.payload_bytes(), b"123");
    }
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_middle() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"temperature\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 11: temperature..."
        );
    }
    #[test]
    fn payload_str_with_invalid_utf8_char_in_the_beginning() {
        let topic = Topic::new("trimmed").unwrap();
        let message = Message::new(&topic, &b"\xc3\x28"[..]);
        assert_eq!(
            message.payload_str().unwrap_err().to_string(),
            "Invalid UTF8 payload: invalid utf-8 sequence of 1 bytes from index 0: ..."
        );
    }
}
