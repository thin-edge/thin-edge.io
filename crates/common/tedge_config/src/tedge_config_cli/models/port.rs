use std::convert::TryFrom;
use std::fmt::Display;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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

impl Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<Port> for u16 {
    fn from(val: Port) -> Self {
        val.0
    }
}

#[cfg(test)]
use assert_matches::*;
#[test]
fn conversion_from_valid_port_succeeds() {
    assert_matches!(Port::try_from("1234".to_string()), Ok(Port(1234)));
}

#[test]
fn conversion_from_longer_integer_fails() {
    assert_matches!(
        Port::try_from("66000".to_string()),
        Err(InvalidPortNumber { .. })
    );
}

#[test]
fn conversion_from_port_to_string() {
    assert_eq!(Port(1234).to_string(), "1234");
}
