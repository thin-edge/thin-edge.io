//! A streaming, almost non-allocating [^1] ThinEdge JSON parser using `serde`.
//!
//! [^1]: It only allocates in presence of escaped strings as keys.
//!
use crate::measurement::MeasurementVisitor;
use serde::de::DeserializeSeed;
use serde::de::MapAccess;
use serde::de::{self};
use serde::Deserializer;
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt;
use time::format_description;
use time::OffsetDateTime;

/// Parses `input` as ThinEdge JSON yielding the parsed measurements to the `visitor`.
pub fn parse_str<T: MeasurementVisitor>(
    input: &str,
    visitor: &mut T,
) -> Result<(), ThinEdgeJsonParserError> {
    let mut deserializer = serde_json::Deserializer::from_str(input);

    let parser = ThinEdgeJsonParser { visitor };

    deserializer
        .deserialize_map(parser)
        .map_err(|error| map_error(error, input))?;
    Ok(())
}

/// The error returned by `parse_str`.
#[derive(Debug, thiserror::Error)]
#[error("Invalid JSON: {error}: `{input_excerpt}`")]
pub struct ThinEdgeJsonParserError {
    /// The underlying serde error.
    error: serde_json::Error,
    /// An excerpt from the input string near the error location.
    input_excerpt: String,
}

/// Parses top-level ThinEdge JSON:
///
/// ```grammar
/// {
///     time?: string,
///     [key: string]: number | {[key: string]: number},
/// }
/// ```
///
struct ThinEdgeJsonParser<'vis, T>
where
    T: MeasurementVisitor,
{
    visitor: &'vis mut T,
}

/// Parses a single value (number) or multi-value measurement:
///
/// ```grammar
/// number | {[key: string]: number}
/// ```
///
struct ThinEdgeValueParser<'key, 'vis, T> {
    /// Recursion depth.
    ///
    /// When `depth = 0`, we accept both number or multi-value measurements.
    /// When `depth > 0`, we only accept numbers.
    depth: usize,
    /// The associated key of the single or multi-value measurement.
    key: Cow<'key, str>,
    /// The visitor to callback into when parsing relevant data.
    visitor: &'vis mut T,
}

impl<'vis, 'de, T> de::Visitor<'de> for ThinEdgeJsonParser<'vis, T>
where
    T: MeasurementVisitor,
{
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("ThinEdge JSON")
    }

    fn visit_map<V>(self, mut map: V) -> Result<(), V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut measurements_count: usize = 0;

        while let Some(key) = map.next_key()? {
            let key: Cow<str> = key;

            match key.as_ref() {
                "time" => {
                    let timestamp_str: &str = map.next_value()?;
                    let timestamp = OffsetDateTime::parse(
                        timestamp_str,
                        &format_description::well_known::Rfc3339,
                    )
                    .map_err(|err| de::Error::custom(invalid_timestamp(timestamp_str, err)))?;

                    self.visitor
                        .visit_timestamp(timestamp)
                        .map_err(de::Error::custom)?;
                }
                _key => {
                    let parser = ThinEdgeValueParser {
                        depth: 0,
                        key,
                        visitor: self.visitor,
                    };

                    map.next_value_seed(parser)?;
                    measurements_count += 1;
                }
            }
        }

        if measurements_count == 0 {
            return Err(de::Error::custom(invalid_empty_root()));
        }

        Ok(())
    }
}

impl<'key, 'vis, 'de, T> de::Visitor<'de> for ThinEdgeValueParser<'key, 'vis, T>
where
    T: MeasurementVisitor,
{
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        if self.depth == 0 {
            formatter.write_str("ThinEdge single or multi-value measurement")
        } else {
            formatter.write_str("ThinEdge single-value measurement")
        }
    }

    /// Parses a multi-value measurement: `{[string]: number}` or fails if depth > 0.
    ///
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        // To support arbitrarily nested measurements remove the following line.
        if self.depth > 0 {
            return Err(de::Error::custom("Expect single-value measurement"));
        }

        self.visitor
            .visit_start_group(self.key.as_ref())
            .map_err(de::Error::custom)?;

        let mut measurements_count: usize = 0;

        while let Some(key) = map.next_key()? {
            let parser = ThinEdgeValueParser {
                depth: self.depth + 1,
                key,
                visitor: self.visitor,
            };

            map.next_value_seed(parser)?;
            measurements_count += 1;
        }

        if measurements_count == 0 {
            return Err(de::Error::custom(invalid_empty_measurement(&self.key)));
        }

        self.visitor.visit_end_group().map_err(de::Error::custom)?;

        Ok(())
    }

    /// Parses a single-value measurement.
    ///
    /// `serde_json` requires us to handle three cases:
    ///     - floating point numbers (f64),
    ///     - negative integers (i64) and
    ///     - positive integers (u64).
    ///
    /// See `visit_i64` and `visit_u64`.
    ///
    /// For JSON `1.0`, serde_json will call `visit_f64`.
    /// For JSON `-31`, serde_json will call `visit_i64`.
    /// For JSON `420`, serde_json will call `visit_u64`.
    ///
    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if value != 0.0 && !value.is_normal() {
            return Err(de::Error::custom(invalid_json_number(&self.key)));
        }

        self.visitor
            .visit_measurement(self.key.as_ref(), value)
            .map_err(de::Error::custom)?;

        Ok(())
    }

    /// Parses a single-value measurement. See `visit_f64`.
    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = i32::try_from(value)
            .map_err(|_| de::Error::custom(invalid_json_number(&self.key)))?
            .into();

        self.visit_f64(value)
    }

    /// Parses a single-value measurement. See `visit_f64`.
    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let value = u32::try_from(value)
            .map_err(|_| de::Error::custom(invalid_json_number(&self.key)))?
            .into();

        self.visit_f64(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visitor
            .visit_text_property(self.key.as_ref(), value)
            .map_err(de::Error::custom)?;

        Ok(())
    }
}

/// The `DeserializeSeed` trait enables us to inject state required for deserialization. In our case
/// the state is the `visitor` that we want to use for callbacks and the `key` that we are currently
/// parsing.
///
/// As we are passing the parsed data over to the embedded visitor, all of our parsers do not
/// produce a value, so we use the empty tuple type.
impl<'key, 'vis, 'de, T> DeserializeSeed<'de> for ThinEdgeValueParser<'key, 'vis, T>
where
    T: MeasurementVisitor,
{
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Use `self` as `de::Visitor`
        deserializer.deserialize_any(self)
    }
}

fn invalid_json_number(key: &str) -> String {
    format!(
        "Number out-of-range: the {:?} value is too large to be represented as a float64.",
        key
    )
}

fn invalid_timestamp(value: &str, err: impl std::fmt::Display) -> String {
    format!(
        "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): {:?}: {}",
        value, err
    )
}

fn invalid_empty_root() -> &'static str {
    "Empty Thin Edge measurement: it must contain at least one measurement"
}

fn invalid_empty_measurement(key: &str) -> String {
    format!(
        "Empty Thin Edge measurement: {:?} must contain at least one measurement",
        key
    )
}

fn map_error(error: serde_json::Error, input: &str) -> ThinEdgeJsonParserError {
    const MAX_INPUT_EXCERPT: usize = 80;
    let input_excerpt =
        crate::utils::excerpt(input, error.line(), error.column(), MAX_INPUT_EXCERPT);
    ThinEdgeJsonParserError {
        error,
        input_excerpt,
    }
}
#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use crate::parser::parse_str;

    #[test]
    fn it_deserializes_thin_edge_json() -> anyhow::Result<()> {
        use crate::builder::ThinEdgeJsonBuilder;
        let input = r#"{
        "time" : "2021-04-30T17:03:14.123+02:00",
        "pressure": 123.4,
        "temperature": 24,
        "coordinate": {
            "x": 1,
            "y": 2.0,
            "z": -42.0
        },
        "escaped\\": 123.0
    }"#;

        let mut builder = ThinEdgeJsonBuilder::default();

        parse_str(input, &mut builder)?;

        let output = builder.done()?;

        assert_eq!(
            output.timestamp,
            Some(datetime!(2021-04-30 17:03:14.123 +02:00))
        );

        assert_eq!(
            output.values,
            vec![
                ("pressure", 123.4).into(),
                ("temperature", 24.0).into(),
                (
                    "coordinate",
                    vec![("x", 1.0).into(), ("y", 2.0).into(), ("z", -42.0).into(),]
                )
                    .into(),
                (r#"escaped\"#, 123.0).into(),
            ]
        );
        Ok(())
    }

    #[test]
    fn it_shows_input_excerpt_on_error() -> anyhow::Result<()> {
        use crate::builder::ThinEdgeJsonBuilder;

        let input = "{\n\"time\" : null\n}";

        let mut builder = ThinEdgeJsonBuilder::default();

        let res = parse_str(input, &mut builder);

        assert!(res.is_err());

        assert_eq!(
        res.unwrap_err().to_string(),
        "Invalid JSON: invalid type: null, expected a borrowed string at line 2 column 13: `l\n}\n`",
    );

        Ok(())
    }

    #[test]
    fn parse_type_as_measurement() -> anyhow::Result<()> {
        use crate::builder::ThinEdgeJsonBuilder;

        let input = r#"{
            "time" : "2021-04-30T17:03:14.123+02:00",
            "pressure": 123.4,
            "type": "456"       
        }"#;

        let mut builder = ThinEdgeJsonBuilder::default();

        let res = parse_str(input, &mut builder);

        assert!(res.is_ok());

        Ok(())
    }
}
