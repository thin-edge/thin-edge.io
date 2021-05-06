use chrono::prelude::*;
use std::io::Write;
use thin_edge_json::{json::ThinEdgeJsonError, measurement::GroupedMeasurementVisitor};

pub struct C8yJsonSerializer {
    buffer: Vec<u8>,
    is_within_group: bool,
    needs_separator: bool,
    timestamp_present: bool,
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
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
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
    pub fn new() -> Result<Self, C8yJsonSerializationError> {
        let mut serializer = C8yJsonSerializer {
            buffer: Vec::new(),
            is_within_group: false,
            needs_separator: true,
            timestamp_present: false,
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
    use serde_json::json;

    use super::*;
    use chrono::{offset::FixedOffset, DateTime, Local};
    fn test_timestamp() -> DateTime<FixedOffset> {
        let local_time_now: DateTime<Local> = Local::now();
        local_time_now.with_timezone(local_time_now.offset())
    }
    #[test]
    fn serialize_single_value_message() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp).unwrap();
        serializer.measurement("temperature", 25.5).unwrap();
        let output = serializer.bytes().unwrap();

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
            "temperature":{
                "temperature":{
                    "value": 25.5
                }
            }
        });

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
            expected_output
        );
    }
    #[test]
    fn serialize_multi_value_message() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp).unwrap();
        serializer.measurement("temperature", 25.5).unwrap();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        serializer.measurement("lati", 2300.4).unwrap();
        serializer.end_group().unwrap();
        serializer.measurement("pressure", 255.2).unwrap();

        let output = serializer.bytes().unwrap();

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
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
            serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
            expected_output
        );
    }

    #[test]
    fn serialize_empty_message() {
        let serializer = C8yJsonSerializer::new().unwrap();
        let expected_output: Vec<u8> = format!(r#"{{"type": "ThinEdgeMeasurement"}}"#).into_bytes();
        let output = serializer.bytes().unwrap();

        assert_eq!(expected_output.to_vec(), output);
    }

    #[test]
    fn serialize_timestamp_message() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp).unwrap();
        let expected_output: Vec<u8> = format!(
            r#"{{"type": "ThinEdgeMeasurement","time":"{}"}}"#,
            timestamp.to_rfc3339()
        )
        .into();
        let output = serializer.bytes().unwrap();
        assert_eq!(expected_output, output);
    }

    #[test]
    fn serialize_timestamp_within_group() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        let timestamp = test_timestamp();
        serializer.start_group("location").unwrap();
        let result = serializer.timestamp(timestamp);
        let expected_error = "Unexpected time stamp within a group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_end_of_group() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let result = serializer.end_group();
        let expected_error = "Unexpected end of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_start_of_group() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let result = serializer.start_group("location2");
        let expected_error = "Unexpected start of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_end_of_message() {
        let mut serializer = C8yJsonSerializer::new().unwrap();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let expected_error = "Unexpected end of data";
        let result = serializer.bytes();
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }
}
