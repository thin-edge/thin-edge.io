use std::fmt;
use std::str::FromStr;

/// A cloud endpoint given as a bare domain name, without a scheme or port
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
pub struct ConnectUrl {
    input: String,
}

impl ConnectUrl {
    pub fn as_str(&self) -> &str {
        &self.input
    }
}

impl FromStr for ConnectUrl {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match url::Host::parse(input) {
            Ok(_) => Ok(Self {
                input: input.to_owned(),
            }),
            Err(error) => Err(format!(
                "Could not parse provided URL: '{input}' as a valid domain name ({error}). \
                 Provided URL should contain only domain, eg: 'subdomain.cumulocity.com'."
            )),
        }
    }
}

impl fmt::Display for ConnectUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.input.fmt(f)
    }
}

impl TryFrom<String> for ConnectUrl {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&ConnectUrl> for String {
    fn from(value: &ConnectUrl) -> Self {
        value.input.clone()
    }
}

tedge_config_engine::impl_single_value_append_remove!(ConnectUrl);
