use std::convert::{TryFrom, TryInto};

#[derive(Debug, Copy, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct Port(pub u16);

#[derive(thiserror::Error, Debug)]
#[error("Invalid port number: '{input}'.")]
pub struct InvalidPortNumber {
    input: String,
}

impl TryFrom<String> for Port {
    type Error = InvalidPortNumber;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        input
            .as_str()
            .parse::<u16>()
            .map_err(|_| InvalidPortNumber { input })
            .map(Port)
    }
}

impl TryInto<String> for Port {
    type Error = std::convert::Infallible;

    fn try_into(self) -> Result<String, Self::Error> {
        Ok(format!("{}", self.0))
    }
}

impl Into<u16> for Port {
    fn into(self) -> u16 {
        self.0
    }
}
