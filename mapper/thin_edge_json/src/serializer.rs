use crate::stream::*;
use chrono::{offset::FixedOffset, DateTime};
use json_writer::{JsonWriter, JsonWriterError};

#[derive(Debug)]
pub struct ThinEdgeJsonSerializer {
    json: JsonWriter,
    state: State,
    default_timestamp: Option<DateTime<FixedOffset>>,
    timestamp_present: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum State {
    /// State before `start` was called.
    NotStarted,
    /// After `start` and when not within a group.
    Started,
    /// Within a group.
    WithinGroup,
    /// After `end` was called.
    Finished,
    /// Error state
    Error,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonSerializationError {
    #[error(transparent)]
    FormatError(#[from] std::fmt::Error),

    #[error(transparent)]
    MeasurementStreamError(#[from] MeasurementStreamError),

    #[error("Serializer produced invalid Utf8 string")]
    InvalidUtf8ConversionToString(std::string::FromUtf8Error),

    #[error(transparent)]
    JsonWriterError(#[from] JsonWriterError),
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementStreamError {
    #[error("Unexpected start of document")]
    UnexpectedStartOfDocument,

    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("Duplicated time stamp")]
    DuplicatedTimestamp,

    #[error("Unexpected measurement")]
    UnexpectedMeasurement,

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
        let capacity = 1024; // XXX: Choose a capacity based on expected JSON length.

        Self {
            json: JsonWriter::with_capacity(capacity),
            state: State::NotStarted,
            default_timestamp,
            timestamp_present: false,
        }
    }

    pub fn bytes(self) -> Result<Vec<u8>, ThinEdgeJsonSerializationError> {
        Ok(self.into_string()?.into_bytes())
    }

    pub fn into_string(mut self) -> Result<String, ThinEdgeJsonSerializationError> {
        match self.state {
            State::Finished => Ok(self.json.into_string()?),
            _ => Err(self.state_error(MeasurementStreamError::UnexpectedEndOfData)),
        }
    }

    fn state_error(&mut self, err: MeasurementStreamError) -> ThinEdgeJsonSerializationError {
        self.state = State::Error;
        err.into()
    }
}

impl Default for ThinEdgeJsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurementStreamConsumer for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;

    fn consume<'a>(&mut self, item: MeasurementStreamItem<'a>) -> Result<(), Self::Error> {
        match (item, &mut self.state) {
            (MeasurementStreamItem::StartDocument, State::NotStarted) => {
                self.json.write_open_obj();
                self.state = State::Started;
                Ok(())
            }
            (MeasurementStreamItem::StartDocument, _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedStartOfDocument))
            }
            (MeasurementStreamItem::EndDocument, State::Started) => {
                if let (false, Some(default_timestamp)) =
                    (self.timestamp_present, self.default_timestamp)
                {
                    let () = self.consume(MeasurementStreamItem::Timestamp(default_timestamp))?;
                }

                self.json.write_close_obj();
                self.state = State::Finished;
                Ok(())
            }
            (MeasurementStreamItem::EndDocument, _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedEndOfData))
            }
            (MeasurementStreamItem::Timestamp(timestamp), State::Started) => {
                if self.timestamp_present {
                    Err(self.state_error(MeasurementStreamError::DuplicatedTimestamp))
                } else {
                    self.json.write_key("time")?;
                    self.json.write_str(timestamp.to_rfc3339().as_str())?;
                    self.timestamp_present = true;
                    Ok(())
                }
            }
            (MeasurementStreamItem::Timestamp(_), _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedTimestamp))
            }
            (MeasurementStreamItem::Measurement { name, value }, State::Started) => {
                self.json.write_key(name)?;
                self.json.write_f64(value)?;
                Ok(())
            }
            (MeasurementStreamItem::Measurement { name, value }, State::WithinGroup) => {
                self.json.write_key(name)?;
                self.json.write_f64(value)?;
                Ok(())
            }
            (MeasurementStreamItem::Measurement { .. }, _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedMeasurement))
            }

            (MeasurementStreamItem::StartGroup(group), State::Started) => {
                self.json.write_key(group)?;
                self.json.write_open_obj();
                self.state = State::WithinGroup;
                Ok(())
            }
            (MeasurementStreamItem::StartGroup(_), _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedStartOfGroup))
            }

            (MeasurementStreamItem::EndGroup, State::WithinGroup) => {
                self.json.write_close_obj();
                self.state = State::Started;
                Ok(())
            }
            (MeasurementStreamItem::EndGroup, _) => {
                Err(self.state_error(MeasurementStreamError::UnexpectedEndOfGroup))
            }
        }
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
    fn serialize_single_value_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        let timestamp = test_timestamp();

        serializer.start()?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.end()?;

        let body = r#""temperature":25.5"#;
        let expected_output = format!(r#"{{"time":"{}",{}}}"#, timestamp.to_rfc3339(), body);
        let output = serializer.inner().into_string()?;
        assert_eq!(output, expected_output);
        Ok(())
    }

    #[test]
    fn serialize_single_value_no_timestamp_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        serializer.measurement("temperature", 25.5)?;
        serializer.end()?;
        let expected_output = r#"{"temperature":25.5}"#;
        let output = serializer.inner().into_string()?;
        assert_eq!(output, expected_output);
        Ok(())
    }

    #[test]
    fn serialize_multi_value_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        let timestamp = test_timestamp();
        serializer.start()?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        serializer.measurement("lati", 2300.4)?;
        serializer.end_group()?;
        serializer.measurement("pressure", 255.0)?;
        serializer.end()?;

        let body = r#""temperature":25.5,"location":{"alti":2100.4,"longi":2200.4,"lati":2300.4},"pressure":255.0}"#;
        let expected_output = format!(r#"{{"time":"{}",{}"#, timestamp.to_rfc3339(), body);
        let output = serializer.inner().into_string()?;
        assert_eq!(expected_output, output);
        Ok(())
    }

    #[test]
    fn serialize_empty_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        serializer.end()?;
        let expected_output = "{}";
        let output = serializer.inner().into_string()?;
        assert_eq!(expected_output, output);
        Ok(())
    }

    #[test]
    fn serialize_timestamp_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        let timestamp = test_timestamp();
        serializer.timestamp(timestamp)?;
        serializer.end()?;
        let expected_output = format!(r#"{{"time":"{}"{}"#, timestamp.to_rfc3339(), "}");
        let output = serializer.inner().into_string()?;
        assert_eq!(expected_output, output);
        Ok(())
    }

    #[test]
    fn serialize_timestamp_within_group() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        let timestamp = test_timestamp();
        serializer.start()?;
        serializer.start_group("location")?;
        let result = serializer.timestamp(timestamp);
        let expected_error = "Unexpected time stamp within a group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }

    #[test]
    fn serialize_unexpected_end_of_group() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        let result = serializer.end_group();
        let expected_error = "Unexpected end of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }

    #[test]
    fn serialize_unexpected_start_of_group() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        let result = serializer.start_group("location");
        let expected_error = "Unexpected start of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }

    #[test]
    fn serialize_unexpected_end_of_message() -> anyhow::Result<()> {
        let mut serializer = StreamBuilder::from(ThinEdgeJsonSerializer::new());
        serializer.start()?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        let expected_error = "Unexpected end of data";
        let result = serializer.inner().into_string();
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }
}
