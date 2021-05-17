use chrono::prelude::*;
use std::io::Write;
use thin_edge_json::{json::ThinEdgeJsonError, measurement::GroupedMeasurementVisitor};

pub struct C8yJsonSerializer {
    buffer: Vec<u8>,
    is_within_group: bool,
    needs_separator: bool,
    timestamp_present: bool,
    default_timestamp: DateTime<FixedOffset>,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error(transparent)]
    ThinEdgeJsonParseError(#[from] ThinEdgeJsonError),
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
    pub fn new(
        default_timestamp: DateTime<FixedOffset>,
    ) -> Result<Self, C8yJsonSerializationError> {
        let mut serializer = C8yJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: true,
            timestamp_present: false,
            default_timestamp,
        };

        let _ = serializer
            .buffer
            .write(b"{\"type\": \"ThinEdgeMeasurement\"")?;
        Ok(serializer)
    }

    fn end(&mut self) -> Result<(), C8yJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        if !self.timestamp_present {
            self.timestamp(self.default_timestamp)?;
        }

        assert!(self.timestamp_present);

        self.buffer.push(b'}');
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, C8yJsonSerializationError> {
        self.end()?;
        Ok(self.buffer)
    }
}

impl GroupedMeasurementVisitor for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer
            .write_fmt(format_args!("\"time\":\"{}\"", timestamp.to_rfc3339()))?;
        self.needs_separator = true;
        self.timestamp_present = true;
        Ok(())
    }

    fn measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(b',');
        } else {
            self.needs_separator = true;
        }
        if self.is_within_group {
            self.buffer
                .write_fmt(format_args!(r#""{}": {{"value": {}}}"#, key, value))?;
        } else {
            self.buffer.write_fmt(format_args!(
                r#""{}": {{"{}": {{"value": {}}}}}"#,
                key, key, value
            ))?;
        }
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.buffer.push(b',');
        }
        self.buffer.write_fmt(format_args!("\"{}\":{{", group))?;
        self.needs_separator = false;
        self.is_within_group = true;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.buffer.push(b'}');
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

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;

        let output = serializer.bytes()?;

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
            serde_json::from_slice::<serde_json::Value>(&output)?,
            expected_output
        );
        Ok(())
    }
    #[test]
    fn serialize_multi_value_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        serializer.measurement("lati", 2300.4)?;
        serializer.end_group()?;
        serializer.measurement("pressure", 255.2)?;

        let output = serializer.bytes()?;

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
            serde_json::from_slice::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_empty_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let serializer = C8yJsonSerializer::new(timestamp)?;

        let expected_output =
            json!({"type": "ThinEdgeMeasurement", "time": "2021-06-22T17:03:14.123456789+05:00"});

        let output = serializer.bytes()?;

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_timestamp_message() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
        serializer.timestamp(timestamp)?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time":"2021-06-22T17:03:14.123456789+05:00"
        });

        let output = serializer.bytes()?;

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }

    #[test]
    fn serialize_timestamp_within_group() -> anyhow::Result<()> {
        let timestamp = FixedOffset::east(5 * 3600)
            .ymd(2021, 6, 22)
            .and_hms_nano(17, 3, 14, 123456789);

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
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

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
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

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
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

        let mut serializer = C8yJsonSerializer::new(timestamp)?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.bytes();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedEndOfData
            ))
        );

        Ok(())
    }
}
