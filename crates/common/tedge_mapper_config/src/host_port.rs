use std::fmt;
use std::str::FromStr;

use tedge_config_engine::AppendRemoveItem;

/// The port HTTPS uses when a host is given without one
pub const HTTPS_PORT: u16 = 443;

/// The port MQTT over TLS uses when a host is given without one
pub const MQTT_TLS_PORT: u16 = 8883;

/// A host with an optional port, using port `P` when none is given
///
/// The input is kept as written, so a file holds what the user set, while
/// `tedge config` shows the host with the port it resolves to.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
// facet can't detect the traits of a generic type, so they are declared
#[facet(traits(Debug, Clone, PartialEq, Display, FromStr))]
pub struct HostPort<const P: u16> {
    input: String,
    host: String,
    port: u16,
}

impl<const P: u16> HostPort<P> {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl<const P: u16> FromStr for HostPort<P> {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let invalid = |e: &dyn fmt::Display| format!("Invalid host '{input}': {e}");
        let (host, port) = match input.split_once(':') {
            Some((host, port)) => (host, port.parse().map_err(|e| invalid(&e))?),
            None => (input, P),
        };
        let host = url::Host::parse(host).map_err(|e| invalid(&e))?;
        Ok(Self {
            input: input.to_owned(),
            host: host.to_string(),
            port,
        })
    }
}

impl<const P: u16> fmt::Display for HostPort<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl<const P: u16> TryFrom<String> for HostPort<P> {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl<const P: u16> From<&HostPort<P>> for String {
    fn from(value: &HostPort<P>) -> Self {
        value.input.clone()
    }
}

impl<const P: u16> AppendRemoveItem for HostPort<P> {
    fn append(_current: Option<Self>, new_value: Self) -> Option<Self> {
        Some(new_value)
    }

    fn remove(current: Option<Self>, remove_value: Self) -> Option<Self> {
        match current {
            Some(v) if v == remove_value => None,
            other => other,
        }
    }
}
