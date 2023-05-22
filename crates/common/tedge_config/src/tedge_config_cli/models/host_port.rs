use crate::ConnectUrl;
use crate::Port;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;
use url::Host;

/// A combination of a host and a port number.
///
/// This type is used for serializing and deserializing host strings with a port
/// optionally present, like `my-tenant.cumulocity.com` or
/// `mqtt.my-tenant.cumulocity.com:1234`. The port can be omitted in the parsed
/// string, because user code can easily derive a reasonable default port number
/// from the context this type is used in, e.g. 443 for HTTPS or 8883 for MQTT
/// TLS. To easily specify this fallback port, a const type parameter is used.
///
/// # Examples
///
/// ```
/// # use tedge_config::{HostPort, Port, HTTPS_PORT};
///
/// // use a fallback port if not present in string
/// let http = HostPort::<HTTPS_PORT>::try_from("my-tenant.cumulocity.com".to_string()).unwrap();
/// assert_eq!(http.port(), Port(HTTPS_PORT));
///
/// // allow port to be overridden using standard `:PORT` notation
/// let http = HostPort::<HTTPS_PORT>::try_from("my-tenant.cumulocity.com:8080".to_string()).unwrap();
/// assert_eq!(http.port(), Port(8080));
///
/// // return error for malformed host strings
/// assert!(HostPort::<HTTPS_PORT>::try_from("my-tenant.cumulocity.com:8080:443".to_string()).is_err());
/// assert!(HostPort::<HTTPS_PORT>::try_from(":8080".to_string()).is_err());
/// assert!(HostPort::<HTTPS_PORT>::try_from("[:::1]:8080".to_string()).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct HostPort<const P: u16> {
    input: String,
    hostname: url::Host,
    port: Port,
}

impl<const P: u16> HostPort<P> {
    /// Returns the hostname.
    ///
    /// A hostname can be either a DNS domain name, an IPv4 address, or an IPv6
    /// address.
    pub fn host(&self) -> &url::Host {
        &self.hostname
    }

    /// Returns the port number.
    ///
    /// If `:PORT` suffix was present when deserializing, this specified port is
    /// used. If not, a fallback port from the const type parameter is used.
    pub fn port(&self) -> Port {
        self.port
    }

    /// Returns a string representation of the host.
    ///
    /// In practice, it just returns the input string used to construct the
    /// struct.
    pub fn as_str(&self) -> &str {
        &self.input
    }
}

impl<const P: u16> From<HostPort<P>> for String {
    fn from(value: HostPort<P>) -> Self {
        value.input
    }
}

impl<const P: u16> fmt::Display for HostPort<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.input.fmt(f)
    }
}

impl<const P: u16> FromStr for HostPort<P> {
    type Err = ParseHostPortError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_owned())
    }
}

impl<const P: u16> doku::Document for HostPort<P> {
    fn ty() -> doku::Type {
        String::ty()
    }
}

impl<const P: u16> TryFrom<String> for HostPort<P> {
    type Error = ParseHostPortError;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        let (hostname, port) = if let Some((hostname, port)) = input.split_once(':') {
            let port = Port(port.parse()?);
            let hostname: Host<String> = Host::parse(hostname)?;
            (hostname, port)
        } else {
            let hostname: Host<String> = Host::parse(&input)?;
            (hostname, Port(P))
        };

        Ok(HostPort {
            input,
            hostname,
            port,
        })
    }
}

impl<const P: u16> From<ConnectUrl> for HostPort<P> {
    fn from(value: ConnectUrl) -> Self {
        HostPort {
            input: value.input,
            hostname: value.host,
            port: Port(P),
        }
    }
}

/// An error which can be returned when parsing a [`HostPort`].
///
/// The parsing can fail when:
/// - host can not be parsed as IPv4/IPv6 addresses or a DNS name
/// - the `[:PORT]` suffix is present and `PORT` can't be parsed as valid `u16`
#[derive(Debug, thiserror::Error)]
pub enum ParseHostPortError {
    #[error("Could not parse hostname")]
    ParseHostname(#[from] url::ParseError),

    #[error("Could not parse port")]
    ParsePort(#[from] ParseIntError),
}
