use crate::append_remove::AppendRemoveItem;
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

/// Hostname plus port where the port can be supplied by a const default.
#[derive(Debug, Clone, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[facet(traits(FromStr, Display, Debug, Clone, PartialEq, Eq))]
#[serde(into = "String", try_from = "String")]
pub struct HostPort<const P: u16> {
    input: String,
    host: String,
    port: u16,
}

/// Parse error for [`HostPort`].
#[derive(Debug)]
pub enum ParseHostPortError {
    ParsePort(ParseIntError),
    EmptyHost,
}

/// Default HTTPS port used by host/port config fields.
pub const HTTPS_PORT: u16 = 443;

impl<const P: u16> fmt::Display for HostPort<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl<const P: u16> FromStr for HostPort<P> {
    type Err = ParseHostPortError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

impl<const P: u16> From<HostPort<P>> for String {
    fn from(value: HostPort<P>) -> Self {
        value.input
    }
}

impl<const P: u16> TryFrom<String> for HostPort<P> {
    type Error = ParseHostPortError;
    fn try_from(input: String) -> Result<Self, Self::Error> {
        let (host, port) = if let Some((h, p)) = input.split_once(':') {
            (h.to_owned(), p.parse::<u16>()?)
        } else {
            (input.clone(), P)
        };
        if host.is_empty() {
            return Err(ParseHostPortError::EmptyHost);
        }
        Ok(HostPort { input, host, port })
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

impl fmt::Display for ParseHostPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ParsePort(e) => write!(f, "Could not parse port: {e}"),
            Self::EmptyHost => write!(f, "Empty hostname"),
        }
    }
}

impl std::error::Error for ParseHostPortError {}

impl From<ParseIntError> for ParseHostPortError {
    fn from(e: ParseIntError) -> Self {
        Self::ParsePort(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_only_uses_default_port() {
        let hp: HostPort<8883> = "mqtt.example.com".parse().unwrap();
        assert_eq!(hp.host, "mqtt.example.com");
        assert_eq!(hp.port, 8883);
        assert_eq!(hp.to_string(), "mqtt.example.com:8883");
    }

    #[test]
    fn explicit_port_overrides_default() {
        let hp: HostPort<443> = "example.com:8080".parse().unwrap();
        assert_eq!(hp.host, "example.com");
        assert_eq!(hp.port, 8080);
        assert_eq!(hp.to_string(), "example.com:8080");
    }

    #[test]
    fn empty_input_is_rejected() {
        let result: Result<HostPort<443>, _> = "".parse();
        assert!(matches!(result, Err(ParseHostPortError::EmptyHost)));
    }

    #[test]
    fn port_only_is_rejected() {
        let result: Result<HostPort<443>, _> = ":8080".parse();
        assert!(matches!(result, Err(ParseHostPortError::EmptyHost)));
    }

    #[test]
    fn invalid_port_is_rejected() {
        let result: Result<HostPort<443>, _> = "example.com:notaport".parse();
        assert!(matches!(result, Err(ParseHostPortError::ParsePort(_))));
    }

    #[test]
    fn into_string_preserves_original_input() {
        let hp: HostPort<443> = "example.com".parse().unwrap();
        let s: String = hp.into();
        assert_eq!(s, "example.com");
    }

    #[test]
    fn into_string_preserves_explicit_port_input() {
        let hp: HostPort<443> = "example.com:9090".parse().unwrap();
        let s: String = hp.into();
        assert_eq!(s, "example.com:9090");
    }
}
