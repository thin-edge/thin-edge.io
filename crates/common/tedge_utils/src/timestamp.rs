use doku::Document;
use serde::de;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::fmt::Formatter;
use strum::Display;
use strum::EnumString;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(
    Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, Document, EnumString, Display,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum TimeFormat {
    #[serde(rename = "rfc-3339", alias = "rfc3339")]
    #[strum(serialize = "rfc-3339", serialize = "rfc3339")]
    Rfc3339,
    Unix,
}

impl TimeFormat {
    /// Converts a JSON encoded thin-edge time value to the selected serialization format
    ///
    /// If the input is a number, this is assumed to be a unix timestamp. If the input is
    /// a string, this is assumed to be RFC-3339 formatted
    pub fn reformat_json(self, value: Value) -> serde_json::Result<Value> {
        match (&value, self) {
            (Value::Number(_), TimeFormat::Unix) => Ok(value),
            (Value::String(_), TimeFormat::Rfc3339) => Ok(value),
            (_, format) => {
                let IsoOrUnix(time) = serde_json::from_value(value)?;
                format.to_json(time)
            }
        }
    }

    /// Converts an [OffsetDateTime] to a `serde_json` [Value] in the selected serialization format
    pub fn to_json(self, offset_date_time: OffsetDateTime) -> serde_json::Result<Value> {
        match self {
            TimeFormat::Unix => Ok((offset_date_time.unix_timestamp_nanos() as f64 / 1e9).into()),
            TimeFormat::Rfc3339 => Ok(offset_date_time
                .format(&Rfc3339)
                .map_err(|e| de::Error::custom(e.to_string()))?
                .into()),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
/// A time that can be deserialized from either a unix timestamp in seconds or an RFC-3339 string
pub struct IsoOrUnix(OffsetDateTime);

impl TryFrom<&Value> for IsoOrUnix {
    type Error = serde_json::Error;

    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        Self::deserialize(value)
    }
}

impl fmt::Debug for IsoOrUnix {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl IsoOrUnix {
    pub fn new(offset_date_time: OffsetDateTime) -> Self {
        Self(offset_date_time)
    }

    pub fn into_inner(self) -> OffsetDateTime {
        self.0
    }
}

impl From<IsoOrUnix> for OffsetDateTime {
    fn from(value: IsoOrUnix) -> Self {
        value.0
    }
}

/// A function that can be used with [serde::deserialize_with](https://serde.rs/field-attrs.html#deserialize_with) to accept
/// either a RFC-3339 formatted date string or a unix timestamp in seconds
pub fn deserialize_optional_string_or_unix_timestamp<'de, D>(
    deserializer: D,
) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: de::Deserializer<'de>,
{
    Option::<IsoOrUnix>::deserialize(deserializer).map(|value| Some(value?.0))
}

struct IsoOrUnixVisitor;

impl<'de> de::Visitor<'de> for IsoOrUnixVisitor {
    type Value = IsoOrUnix;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        formatter.write_str("a date formatted as a unix timestamp (as an integer number of seconds) or an ISO-8601 string")
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        OffsetDateTime::from_unix_timestamp(v)
            .map(IsoOrUnix)
            .map_err(|err| de::Error::custom(invalid_unix_timestamp_int(v, err)))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(v.try_into().map_err(|err| de::Error::custom(format!("invalid unix timestamp, expecting a 64 bit signed integer. provided value ({v}) is too large: {err}")))?)
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        OffsetDateTime::from_unix_timestamp_nanos((v * 1000.0) as i128 * 1_000_000)
            .map(IsoOrUnix)
            .map_err(|err| de::Error::custom(invalid_unix_timestamp_decimal(v, err)))
    }

    fn visit_str<E>(self, timestamp_str: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_borrowed_str(timestamp_str)
    }

    fn visit_borrowed_str<E>(self, timestamp_str: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        OffsetDateTime::parse(timestamp_str, &Rfc3339)
            .map(IsoOrUnix)
            .map_err(|err| de::Error::custom(invalid_iso8601(timestamp_str, err)))
    }
}

impl<'de> Deserialize<'de> for IsoOrUnix {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(IsoOrUnixVisitor)
    }
}

fn invalid_unix_timestamp_decimal(value: f64, err: impl fmt::Display) -> String {
    format!(
        "Invalid unix timestamp (reading decimal value in seconds): {:?}; {}",
        value, err
    )
}

fn invalid_unix_timestamp_int(value: i64, err: impl fmt::Display) -> String {
    format!(
        "Invalid unix timestamp (reading integer value in seconds): {:?}; {}",
        value, err
    )
}

fn invalid_iso8601(value: &str, err: impl fmt::Display) -> String {
    format!(
        "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): {:?}: {}",
        value, err
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_format_deserialize() {
        assert_eq!(
            deserialize_json_and_fromstr("rfc-3339"),
            TimeFormat::Rfc3339
        );
    }

    #[test]
    fn time_format_deserialize_no_hyphen() {
        assert_eq!(deserialize_json_and_fromstr("rfc3339"), TimeFormat::Rfc3339);
    }

    #[test]
    fn time_format_deserialize_unix() {
        assert_eq!(deserialize_json_and_fromstr("unix"), TimeFormat::Unix);
    }

    fn deserialize_json_and_fromstr(input: &str) -> TimeFormat {
        let from_json = serde_json::from_value::<TimeFormat>(serde_json::json!(input)).unwrap();
        let from_str = input.parse::<TimeFormat>().unwrap();
        assert_eq!(from_json, from_str, "Output of JSON (serde::Deserialize) and FromStr (strum::EnumString) deserialization is different");
        from_json
    }
}
