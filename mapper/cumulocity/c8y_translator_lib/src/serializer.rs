use chrono::prelude::*;
use json_writer::{JsonWriter, JsonWriterError};
use std::convert::TryInto;
use thin_edge_json::{json::ThinEdgeJsonError, measurement::GroupedMeasurementVisitor};

pub struct C8yJsonSerializer {
    json: JsonWriter,
    is_within_group: bool,
    needs_separator: bool,
    timestamp_present: bool,
    default_timestamp: DateTime<FixedOffset>,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    FormatError(#[from] std::fmt::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error(transparent)]
    ThinEdgeJsonParseError(#[from] ThinEdgeJsonError),

    #[error("Serializer produced invalid Utf8 string")]
    InvalidUtf8ConversionToString(std::string::FromUtf8Error),

    #[error(transparent)]
    JsonWriterError(#[from] JsonWriterError),
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum MeasurementStreamError {
    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("Unexpected end of data")]
    UnexpectedEndOfData,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,
}

impl C8yJsonSerializer {
    pub fn new(default_timestamp: DateTime<FixedOffset>) -> Self {
        let capa = 1024; // XXX: Choose a capacity based on expected JSON length.
        let mut json = JsonWriter::with_capacity(capa);

        json.write_open_obj();
        json.write_static_key("type");
        json.write_static_str_noescape("ThinEdgeMeasurement");

        Self {
            json,
            is_within_group: false,
            needs_separator: true,
            timestamp_present: false,
            default_timestamp,
        }
    }

    fn end(&mut self) -> Result<(), C8yJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        if !self.timestamp_present {
            self.timestamp(self.default_timestamp)?;
        }

        assert!(self.timestamp_present);

        self.json.write_close_obj();
        Ok(())
    }

    fn write_value_obj(&mut self, value: f64) -> Result<(), C8yJsonSerializationError> {
        self.json.write_open_obj();
        self.json.write_key_noescape("value".try_into()?);
        self.json.write_f64(value)?;
        self.json.write_close_obj();
        Ok(())
    }

    pub fn into_string(&mut self) -> Result<String, C8yJsonSerializationError> {
        self.end()?;
        Ok(self.json.clone().into_string())
    }
}

impl GroupedMeasurementVisitor for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.json.write_separator();
        }

        self.json.write_key_noescape("time".try_into()?);
        self.json
            .write_str_noescape(timestamp.to_rfc3339().as_str().try_into()?);

        self.needs_separator = true;
        self.timestamp_present = true;
        Ok(())
    }

    fn measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.json.write_separator();
        } else {
            self.needs_separator = true;
        }

        self.json.write_key_noescape(key.try_into()?);

        if self.is_within_group {
            self.write_value_obj(value)?;
        } else {
            self.json.write_open_obj();
            self.json.write_key_noescape(key.try_into()?);
            self.write_value_obj(value)?;
            self.json.write_close_obj();
        }
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.json.write_separator();
        }
        self.json.write_key_noescape(group.try_into()?);
        self.json.write_open_obj();
        self.needs_separator = false;
        self.is_within_group = true;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.json.write_close_obj();
        self.needs_separator = true;
        self.is_within_group = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;

    use super::*;
    use chrono::offset::FixedOffset;

    #[test]
    fn serialize_single_value_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;

        let output = serializer.into_string()?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": "2021-06-22T17:03:14.123456789+05:00",
            "temperature":{
                "temperature":{
                    "value": 25.5
                }
            }
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&output)?,
            expected_output
        );
        Ok(())
    }
    #[test]
    fn serialize_multi_value_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        serializer.measurement("lati", 2300.4)?;
        serializer.end_group()?;
        serializer.measurement("pressure", 255.2)?;

        let output = serializer.into_string()?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": "2021-06-22T17:03:14.123456789+05:00",
            "temperature":{
                "temperature":{
                    "value": 25.5
                }
            },
             "location": {
                 "alti": {
                     "value": 2100.4
                 },
                 "longi":{
                     "value": 2200.4
                 },
                 "lati":{
                     "value": 2300.4
                 },
             },
             "pressure":{
                 "pressure":{
                     "value":255.2
                 }
             }

        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_empty_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);

        let expected_output =
            json!({"type": "ThinEdgeMeasurement", "time": "2021-06-22T17:03:14.123456789+05:00"});

        let output = serializer.into_string()?;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_timestamp_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.timestamp(timestamp)?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time":"2021-06-22T17:03:14.123456789+05:00"
        });

        let output = serializer.into_string()?;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_timestamp_within_group() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.start_group("location")?;

        let expected_err = serializer.timestamp(timestamp);

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedTimestamp
            ))
        );
        Ok(())
    }

    #[test]
    fn serialize_unexpected_end_of_group() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.end_group();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedEndOfGroup
            ))
        );

        Ok(())
    }

    #[test]
    fn serialize_unexpected_start_of_group() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.start_group("location2");

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedStartOfGroup
            ))
        );

        Ok(())
    }

    #[test]
    fn serialize_unexpected_end_of_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp);
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.into_string();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedEndOfData
            ))
        );

        Ok(())
    }
}
