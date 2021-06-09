use chrono::prelude::*;
use thin_edge_json::{json::ThinEdgeJsonError, measurement::GroupedMeasurementVisitor};

pub struct C8yJsonSerializer {
    buffer: String,
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
        let mut buffer = String::with_capacity(capa);

        buffer.push_str(r#"{"type": "ThinEdgeMeasurement""#);

        Self {
            buffer,
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

        self.buffer.push('}');
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, C8yJsonSerializationError> {
        self.end()?;
        Ok(self.buffer.into())
    }

    fn write_key(&mut self, key: &str) {
        self.write_str(key);
        self.buffer.push(':');
    }

    fn write_str(&mut self, s: &str) {
        self.buffer.push('"');
        self.buffer.push_str(s);
        self.buffer.push('"');
    }

    fn write_f64(&mut self, value: f64) -> std::fmt::Result {
        use std::fmt::Write;
        self.buffer.write_fmt(format_args!("{}", value))
    }

    fn write_value_obj(&mut self, value: f64) -> std::fmt::Result {
        self.buffer.push('{');
        self.write_key("value");
        self.write_f64(value)?;
        self.buffer.push('}');
        Ok(())
    }
}

impl GroupedMeasurementVisitor for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn timestamp(&mut self, timestamp: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        if self.needs_separator {
            self.buffer.push(',');
        }

        self.write_key("time");
        self.write_str(timestamp.to_rfc3339().as_str());

        self.needs_separator = true;
        self.timestamp_present = true;
        Ok(())
    }

    fn measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(',');
        } else {
            self.needs_separator = true;
        }

        self.write_key(key);

        if self.is_within_group {
            self.write_value_obj(value)?;
        } else {
            self.buffer.push('{');
            self.write_key(key);
            self.write_value_obj(value)?;
            self.buffer.push('}');
        }
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        if self.needs_separator {
            self.buffer.push(',');
        }
        self.write_key(group);
        self.buffer.push('{');
        self.needs_separator = false;
        self.is_within_group = true;
        Ok(())
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.buffer.push('}');
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

        let mut serializer = C8yJsonSerializer::new(timestamp);
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

        let serializer = C8yJsonSerializer::new(timestamp);

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

        let mut serializer = C8yJsonSerializer::new(timestamp);
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
