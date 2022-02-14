use mockall::automock;
use serde::{Deserialize, Deserializer};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub type Timestamp = OffsetDateTime;

#[automock]
pub trait Clock: Sync + Send + 'static {
    fn now(&self) -> Timestamp;
}

#[derive(Clone)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> Timestamp {
        OffsetDateTime::now_utc()
    }
}

pub fn deserialize_iso8601_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let timestamp = String::deserialize(deserializer)?;
    OffsetDateTime::parse(timestamp.as_str(), &Rfc3339)
        .map_err(serde::de::Error::custom)
        .map(Some)
}
