use std::convert::{TryFrom, TryInto};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Deserialize, Serialize)]
pub struct Buffer(pub u16);

#[derive(thiserror::Error, Debug)]
#[error("Invalid buffer size: '{input}'.")]
pub struct InvalidBufferSizeInPercentage {
    input: String,
}

impl TryFrom<String> for Buffer {
    type Error = InvalidBufferSizeInPercentage;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        input
            .as_str()
            .parse::<u16>()
            .map_err(|_| InvalidBufferSizeInPercentage { input })
            .map(Buffer)
    }
}

impl TryInto<String> for Buffer {
    type Error = std::convert::Infallible;
    fn try_into(self) -> Result<String, Self::Error> {
        Ok(format!("{}", self.0))
    }
}

impl From<Buffer> for u16 {
    fn from(val: Buffer) -> Self {
        if val.0 > 100 {
            panic!("Percentage value must be between 0 and 100");
        }
        val.0
    }
}


#[cfg(test)]
use assert_matches::*;
#[test]
fn conversion_from_valid_buffer_size_succeeds() {
    assert_matches!(Buffer::try_from("10".to_string()), Ok(Buffer(10)));
}

#[test]
fn conversion_from_longer_float_fails() {
    assert_matches!(
        Buffer::try_from("66000".to_string()),
        Err(InvalidBufferSizeInPercentage { .. })
    );
}

#[test]
fn conversion_from_port_to_string() {
    assert_matches!(TryInto::<String>::try_into(Buffer(1234)), Ok(buffer_str) if buffer_str == "1234");
}
