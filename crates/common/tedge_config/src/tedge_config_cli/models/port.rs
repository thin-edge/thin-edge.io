use std::convert::TryFrom;
use std::convert::TryInto;

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

impl TryInto<String> for Port {
    type Error = std::convert::Infallible;

    fn try_into(self) -> Result<String, Self::Error> {
        Ok(format!("{}", self.0))
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
    assert_matches!(TryInto::<String>::try_into(Port(1234)), Ok(port_str) if port_str == "1234");
}
