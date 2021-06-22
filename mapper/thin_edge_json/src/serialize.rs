use chrono::offset::FixedOffset;
use chrono::DateTime;

use crate::measurement::GroupedMeasurementVisitor;
pub struct ThinEdgeJsonSerializer {
    buffer: String,
    is_within_group: bool,
    needs_separator: bool,
    default_timestamp: Option<DateTime<FixedOffset>>,
    timestamp_present: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonSerializationError {
    #[error(transparent)]
    FormatError(#[from] std::fmt::Error),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error("Serializer produced invalid Utf8 string")]
    InvalidUtf8ConversionToString(std::string::FromUtf8Error),
}

#[derive(thiserror::Error, Debug)]
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

impl ThinEdgeJsonSerializer {
    pub fn new() -> Self {
        Self::new_with_timestamp(None)
    }

    pub fn new_with_timestamp(default_timestamp: Option<DateTime<FixedOffset>>) -> Self {
        let capa = 1024; // XXX: Choose a capacity based on expected JSON length.
        let mut buffer = String::with_capacity(capa);
        buffer.push('{');

        Self {
            buffer,
            is_within_group: false,
            needs_separator: false,
            default_timestamp,
            timestamp_present: false,
        }
    }

    fn end(&mut self) -> Result<(), ThinEdgeJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        if !self.timestamp_present {
            if let Some(default_timestamp) = self.default_timestamp {
                let () = self.timestamp(default_timestamp)?;
            }
        }

        self.buffer.push('}');
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, ThinEdgeJsonSerializationError> {
        self.end()?;
        Ok(self.buffer.into())
    }

    // XXX: We need to abstract all this into a JsonSerializer.
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

    pub fn into_string(self) -> Result<String, ThinEdgeJsonSerializationError> {
        String::from_utf8(self.bytes()?)
            .map_err(ThinEdgeJsonSerializationError::InvalidUtf8ConversionToString)
    }
}

impl Default for ThinEdgeJsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl GroupedMeasurementVisitor for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;

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

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if self.needs_separator {
            self.buffer.push(',');
        }
        self.write_key(name);
        self.write_f64(value)?;
        self.needs_separator = true;
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
    use super::*;
    use chrono::{offset::FixedOffset, DateTime, Local};
    fn test_timestamp() -> DateTime<FixedOffset> {
        let local_time_now: DateTime<Local> = Local::now();
        local_time_now.with_timezone(local_time_now.offset())
    }

    #[test]
    fn serialize_single_value_message() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();

        serializer.timestamp(timestamp).unwrap();
        serializer.measurement("temperature", 25.5).unwrap();

        let body = r#""temperature":25.5"#;
        let expected_output: Vec<u8> =
            format!(r#"{{"time":"{}",{}}}"#, timestamp.to_rfc3339(), body).into();
        let output = serializer.bytes().unwrap();
        assert_eq!(output, expected_output);
    }

    #[test]
    fn serialize_single_value_no_timestamp_message() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.measurement("temperature", 25.5).unwrap();
        let expected_output = b"{\"temperature\":25.5}";
        let output = serializer.bytes().unwrap();
        assert_eq!(output, expected_output);
    }

    #[test]
    fn serialize_multi_value_message() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp).unwrap();
        serializer.measurement("temperature", 25.5).unwrap();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        serializer.measurement("lati", 2300.4).unwrap();
        serializer.end_group().unwrap();
        serializer.measurement("pressure", 255.0).unwrap();
        let body = r#""temperature":25.5,"location":{"alti":2100.4,"longi":2200.4,"lati":2300.4},"pressure":255}"#;
        let expected_output: Vec<u8> =
            format!(r#"{{"time":"{}",{}"#, timestamp.to_rfc3339(), body).into();
        let output = serializer.bytes().unwrap();
        assert_eq!(expected_output, output);
    }

    #[test]
    fn serialize_empty_message() {
        let serializer = ThinEdgeJsonSerializer::new();
        let expected_output = b"{}";
        let output = serializer.bytes().unwrap();
        assert_eq!(expected_output.to_vec(), output);
    }

    #[test]
    fn serialize_timestamp_message() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp).unwrap();
        let expected_output: Vec<u8> =
            format!(r#"{{"time":"{}"{}"#, timestamp.to_rfc3339(), "}").into();
        let output = serializer.bytes().unwrap();
        assert_eq!(expected_output, output);
    }

    #[test]
    fn serialize_timestamp_within_group() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.start_group("location").unwrap();
        let result = serializer.timestamp(timestamp);
        let expected_error = "Unexpected time stamp within a group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_end_of_group() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let result = serializer.end_group();
        let expected_error = "Unexpected end of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_start_of_group() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let result = serializer.start_group("location");
        let expected_error = "Unexpected start of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }

    #[test]
    fn serialize_unexpected_end_of_message() {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.start_group("location").unwrap();
        serializer.measurement("alti", 2100.4).unwrap();
        serializer.measurement("longi", 2200.4).unwrap();
        let expected_error = "Unexpected end of data";
        let result = serializer.bytes();
        assert_eq!(expected_error, result.unwrap_err().to_string());
    }
}
