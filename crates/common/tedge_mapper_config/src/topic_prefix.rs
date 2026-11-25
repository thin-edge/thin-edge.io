use std::fmt;
use std::str::FromStr;

/// A valid MQTT topic prefix, used to customise the c8y/ topic prefix
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
pub struct TopicPrefix(String);

impl TopicPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for TopicPrefix {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() {
            return Err("Topic prefix must not be empty".into());
        }
        if value.contains('#') {
            return Err("Topic prefix cannot contain '#'".into());
        }
        if value.contains('+') {
            return Err("Topic prefix cannot contain '+'".into());
        }
        if value == "c8y-internal" {
            return Err("Topic prefix cannot be c8y-internal".into());
        }
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for TopicPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for TopicPrefix {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&TopicPrefix> for String {
    fn from(value: &TopicPrefix) -> Self {
        value.0.clone()
    }
}

tedge_config_engine::impl_single_value_append_remove!(TopicPrefix);
