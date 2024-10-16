use std::fmt::Display;

use serde::Serialize;
use tracing::error;

use super::message::MAX_PAYLOAD_LIMIT_IN_BYTES;

/// A Cumulocity SmartREST message payload.
///
/// A SmartREST message is either an HTTP request or an MQTT message that contains SmartREST topic and payload. The
/// payload is a CSV-like format that is backed by templates, either static or registered by the user. This struct
/// represents that payload, and should be used as such in SmartREST 1.0 and 2.0 message implementations.
///
/// # Example
///
/// ```text
/// 503,c8y_Command,"This is a ""Set operation to SUCCESSFUL (503)"" message payload; it has a template id (503),
/// operation fragment (c8y_Command), and optional parameters."
/// ```
///
/// # Reference
///
/// - https://cumulocity.com/docs/smartrest/smartrest-introduction/
#[derive(Debug, Clone, PartialEq, Eq)]
// TODO: pub(crate) for now so it can be constructed manually in serializer::succeed_operation, need to figure out a
// good API
pub struct SmartrestPayload(pub(crate) String);

impl SmartrestPayload {
    /// Creates a payload that consists of a single record.
    ///
    /// Doesn't trim any fields, so if the resulting payload is above size limit, returns an error.
    pub fn serialize<S: Serialize>(record: S) -> Result<Self, SmartrestPayloadError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.serialize(record)?;
        let mut vec = wtr.into_inner().unwrap();

        // remove newline character
        vec.pop();

        let payload = String::from_utf8(vec).expect("csv::Writer should never write invalid utf-8");

        if payload.len() > MAX_PAYLOAD_LIMIT_IN_BYTES {
            return Err(SmartrestPayloadError::TooLarge(payload.len()));
        }

        Ok(Self(payload))
    }

    /// Returns a string slice view of the payload.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Moves the underlying `String` out of the payload.
    pub fn into_inner(self) -> String {
        self.0
    }
}

/// Errors that can occur when trying to create a SmartREST payload.
#[derive(Debug, thiserror::Error)]
pub enum SmartrestPayloadError {
    #[error("Payload size ({0}) would be bigger than the limit ({MAX_PAYLOAD_LIMIT_IN_BYTES})")]
    TooLarge(usize),

    #[error("Could not serialize the record")]
    SerializeError(#[from] csv::Error),
}

impl From<SmartrestPayload> for Vec<u8> {
    fn from(value: SmartrestPayload) -> Self {
        value.0.into_bytes()
    }
}

impl Display for SmartrestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_payload() {
        let payload = SmartrestPayload::serialize((121, true)).unwrap();
        assert_eq!(payload.as_str(), "121,true");
    }

    #[test]
    fn returns_err_when_over_size_limit() {
        let payload = "A".repeat(MAX_PAYLOAD_LIMIT_IN_BYTES + 1);
        let payload = SmartrestPayload::serialize(payload);
        assert!(matches!(payload, Err(SmartrestPayloadError::TooLarge(_))))
    }
}
