use serde::Deserialize;
use serde::Serialize;

pub use super::frame1::Frame1;
pub use super::frame1::ProtocolError;

/// The actual frame that we serialize and send/receive.
///
/// This essentially just adds a version tag and should deal with cases when non-backwards
/// compatible changes are added to new versions.
///
/// For example, current connection semantics is one request/response per connection (client
/// connects, sends request and closes sending half, server reads, sends response and closes sending
/// half, etc.) but if we wanted to move away from that model, we can very easily because the
/// version is the first byte sent by the client so maintaining compatibility should be easy.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame {
    Version1(Frame1),
}

impl Frame {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        postcard::from_bytes::<Frame>(bytes).map_err(|err| {
            let err = anyhow::Error::from(err);
            if bytes.len() < 2 {
                err.context("Frame too short")
            // should be updated if new frame versions are added
            // on nightly, we could use std::mem::variant_count
            } else if let Some(1..) = bytes.get(0) {
                err.context("Unsupported frame version")
            // should be updated if new commands are added
            } else if let Some(14..) = bytes.get(1) {
                err.context("Received request type is not recognized")
            } else {
                err
            }
        })
    }
}

/// Documents the properties of the serialization/deserialization format for purposes of adding new requests while
/// maintaining compatibility.
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Debug)]
    enum F1 {
        V1 { a: u32 },
    }

    #[derive(Serialize, Deserialize, Debug)]
    enum F2 {
        V1 { a: u32, b: u16 },
    }

    /// Shows that when a new field is added to a request, it can't be deserialized if sent as older type.
    #[test]
    fn cant_deserialize_older_as_newer_when_new_fields() {
        let f1 = F1::V1 { a: 42 };
        let f1 = postcard::to_allocvec(&f1).unwrap();
        let f2 = postcard::from_bytes::<F2>(&f1);

        f2.expect_err("can't deserialize older type into newer type");
    }

    /// Shows that when a new field is added to a request, new type can still be deserialized as the older type (new
    /// fields are appended at the end but are ignored by the deserializer).
    #[test]
    fn can_deserialize_newer_as_older_when_new_fields_added_after() {
        let f2 = F2::V1 { a: 42, b: 9 };
        let f2 = postcard::to_allocvec(&f2).unwrap();
        let f1 = postcard::from_bytes::<F1>(&f2);

        f1.unwrap();
    }

    #[derive(Serialize, Deserialize, Debug)]
    enum F3 {
        V1 { b: u16, a: u32 },
    }

    /// postcard is not self-describing, so fields can be confused if new fields are added before older ones
    #[test]
    fn cant_deserialize_newer_as_older_when_new_fields_added_before() {
        let f3 = F3::V1 { a: 42, b: 9 };
        let bytes = postcard::to_allocvec(&f3).unwrap();
        let f1 = postcard::from_bytes::<F1>(&bytes).unwrap();

        let F3::V1 { a: f3_a, .. } = f3;
        let F1::V1 { a: f1_a, .. } = f1;

        // fields are serialized without identifiers, and with the order as defined in the type, so F3::V1.b becomes
        // F1::V1.a, because it's the first number
        assert_eq!(f3_a, 42);
        assert_eq!(f1_a, 9);
    }

    #[test]
    fn deserialize_demo() {
        let frame = Frame::Version1(Frame1::Error(ProtocolError("aaaa".to_string())));
        let bytes = postcard::to_allocvec(&frame).unwrap();

        postcard::from_bytes::<ProtocolError>(&bytes[2..]).unwrap();
        postcard::from_bytes::<Frame1>(&bytes[1..]).unwrap();
        postcard::from_bytes::<Frame>(&bytes).unwrap();
        dbg!(bytes);

        let a = "Hello, World!".as_bytes();
        let mut b = [0u8; 5];
        let b_len = b.len();
        b.copy_from_slice(&a[..b_len]);
    }
}
