use chrono::{DateTime, FixedOffset, Local};
use mockall::automock;
use serde::{Deserialize, Deserializer};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub type Timestamp = DateTime<FixedOffset>;

#[automock]
pub trait Clock: Sync + Send + 'static {
    fn now(&self) -> Timestamp;
}

#[derive(Clone)]
pub struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> Timestamp {
        let local_time_now = Local::now();
        local_time_now.with_timezone(local_time_now.offset())
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
        .map(|val| Some(val))
}
