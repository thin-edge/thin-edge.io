use std::borrow::Cow;
use std::fmt;
use std::fmt::Formatter;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::ensure;
use doku::Document;
use doku::Type;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(try_from = "Cow<'_, str>", into = "Arc<str>")]
/// A valid MQTT topic prefix, used to customise the c8y/ topic prefix
pub struct TopicPrefix(Arc<str>);

impl Document for TopicPrefix {
    fn ty() -> Type {
        String::ty()
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct InvalidTopicPrefix(#[from] anyhow::Error);

impl<'a> TryFrom<Cow<'a, str>> for TopicPrefix {
    type Error = InvalidTopicPrefix;
    fn try_from(value: Cow<'a, str>) -> Result<Self, Self::Error> {
        Self::try_new(&value).map_err(InvalidTopicPrefix)
    }
}

impl TryFrom<&str> for TopicPrefix {
    type Error = InvalidTopicPrefix;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_new(value).map_err(InvalidTopicPrefix)
    }
}

impl TopicPrefix {
    pub fn try_new(value: &str) -> Result<Self, anyhow::Error> {
        ensure!(!value.is_empty(), "Topic prefix must not be empty");
        ensure!(!value.contains('#'), "Topic prefix cannot contain '#'");
        ensure!(!value.contains('+'), "Topic prefix cannot contain '+'");
        ensure!(
            value != "c8y-internal",
            "Topic prefix cannot be c8y-internal"
        );
        Ok(Self(value.into()))
    }
}

impl FromStr for TopicPrefix {
    type Err = InvalidTopicPrefix;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s).map_err(InvalidTopicPrefix)
    }
}

impl From<TopicPrefix> for Arc<str> {
    fn from(value: TopicPrefix) -> Self {
        value.0
    }
}

impl Deref for TopicPrefix {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TopicPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TopicPrefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
