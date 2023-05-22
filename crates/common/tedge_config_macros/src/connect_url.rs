use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;
use url::Host;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct ConnectUrl {
    input: String,
    host: Host,
}

impl doku::Document for ConnectUrl {
    fn ty() -> doku::Type {
        String::ty()
    }
}

#[derive(thiserror::Error, Debug)]
#[error(
    "Provided URL: '{input}' contains scheme or port.
         Provided URL should contain only domain, eg: 'subdomain.cumulocity.com'."
)]
pub struct InvalidConnectUrl {
    input: String,
    error: url::ParseError,
}

impl TryFrom<String> for ConnectUrl {
    type Error = InvalidConnectUrl;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        match Host::parse(&input) {
            Ok(host) => Ok(Self { input, host }),
            Err(error) => Err(InvalidConnectUrl { input, error }),
        }
    }
}

impl FromStr for ConnectUrl {
    type Err = InvalidConnectUrl;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        ConnectUrl::try_from(input.to_string())
    }
}

impl ConnectUrl {
    pub fn as_str(&self) -> &str {
        self.input.as_str()
    }
}

impl fmt::Display for ConnectUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.input.fmt(f)
    }
}

impl From<ConnectUrl> for String {
    fn from(val: ConnectUrl) -> Self {
        val.input
    }
}

impl From<ConnectUrl> for Host {
    fn from(val: ConnectUrl) -> Self {
        val.host
    }
}

#[test]
fn connect_url_from_string_should_return_err_provided_address_with_port() {
    let input = "test.address.com:8883";

    assert!(ConnectUrl::from_str(input).is_err());
}

#[test]
fn connect_url_from_string_should_return_err_provided_address_with_scheme_http() {
    let input = "http://test.address.com";

    assert!(ConnectUrl::from_str(input).is_err());
}

#[test]
fn connect_url_from_string_should_return_err_provided_address_with_port_and_http() {
    let input = "http://test.address.com:8883";

    assert!(ConnectUrl::from_str(input).is_err());
}

#[test]
fn connect_url_from_string_should_return_string() {
    let input = "test.address.com";
    let expected = "test.address.com";

    assert_eq!(ConnectUrl::from_str(input).unwrap().as_str(), expected);
}
