use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

/// An IP address, written to files as a string
///
/// facet_toml can't write an [IpAddr] itself, so this carries one through a
/// string, as tedge_config's files hold it
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
pub struct IpAddress(pub IpAddr);

impl FromStr for IpAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse()
            .map(Self)
            .map_err(|e| format!("'{s}' is not an IP address: {e}"))
    }
}

impl fmt::Display for IpAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for IpAddress {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&IpAddress> for String {
    fn from(value: &IpAddress) -> Self {
        value.to_string()
    }
}

tedge_config_engine::impl_single_value_append_remove!(IpAddress);
