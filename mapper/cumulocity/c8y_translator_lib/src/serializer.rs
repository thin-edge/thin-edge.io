use chrono::prelude::*;
use json_writer::{JsonWriter, JsonWriterError};
use thin_edge_json::{json::ThinEdgeJsonError, stream::*};

#[derive(Debug)]
pub struct C8yJsonSerializer {
    json: JsonWriter,
    state: State,
    timestamp_present: bool,
    default_timestamp: DateTime<FixedOffset>,
}

/// The internal state of the serializer.
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
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    FormatError(#[from] std::fmt::Error),

    #[error(transparent)]
    MeasurementStreamError(#[from] MeasurementStreamError),

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

    #[error("Unexpected start of document")]
    UnexpectedStartOfDocument,

    #[error("Unexpected end of data")]
    UnexpectedEndOfData,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,

    #[error("Unexpected measurement")]
    UnexpectedMeasurement,
}

impl C8yJsonSerializer {
    pub fn new(default_timestamp: DateTime<FixedOffset>) -> Self {
        let capacity = 1024; // XXX: Choose a capacity based on expected JSON length.
        Self {
            json: JsonWriter::with_capacity(capacity),
            state: State::NotStarted,
            timestamp_present: false,
            default_timestamp,
        }
    }

    fn write_value_obj(&mut self, value: f64) -> Result<(), C8yJsonSerializationError> {
        self.json.write_open_obj();
        self.json.write_key("value")?;
        self.json.write_f64(value)?;
        self.json.write_close_obj();
        Ok(())
    }

    pub fn into_string(self) -> Result<String, C8yJsonSerializationError> {
        match self.state {
            State::Finished => Ok(self.json.into_string()?),
            _ => Err(MeasurementStreamError::UnexpectedEndOfData.into()),
        }
    }
}

impl MeasurementStreamConsumer for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn consume<'a>(&mut self, item: MeasurementStreamItem<'a>) -> Result<(), Self::Error> {
        match (item, self.state) {
            (MeasurementStreamItem::StartDocument, State::NotStarted) => {
                self.json.write_open_obj();
                self.json.write_key("type")?;
                self.json.write_str("ThinEdgeMeasurement")?;
                self.state = State::Started;
                Ok(())
            }
            (MeasurementStreamItem::StartDocument, _) => {
                Err(MeasurementStreamError::UnexpectedStartOfDocument.into())
            }
            (MeasurementStreamItem::EndDocument, State::Started) => {
                if !self.timestamp_present {
                    self.consume(MeasurementStreamItem::Timestamp(self.default_timestamp))?;
                }

                assert!(self.timestamp_present);

                self.json.write_close_obj();
                self.state = State::Finished;
                Ok(())
            }
            (MeasurementStreamItem::EndDocument, _) => {
                Err(MeasurementStreamError::UnexpectedEndOfData.into())
            }
            (MeasurementStreamItem::Timestamp(timestamp), State::Started) => {
                self.json.write_key("time")?;
                self.json.write_str(timestamp.to_rfc3339().as_str())?;

                self.timestamp_present = true;
                Ok(())
            }
            (MeasurementStreamItem::Timestamp(_), _) => {
                Err(MeasurementStreamError::UnexpectedTimestamp.into())
            }

            (MeasurementStreamItem::Measurement { name, value }, State::Started) => {
                self.json.write_key(name)?;
                self.json.write_open_obj();
                self.json.write_key(name)?;
                self.write_value_obj(value)?;
                self.json.write_close_obj();
                Ok(())
            }
            (MeasurementStreamItem::Measurement { name, value }, State::WithinGroup) => {
                self.json.write_key(name)?;
                self.write_value_obj(value)?;
                Ok(())
            }

            (MeasurementStreamItem::Measurement { .. }, _) => {
                Err(MeasurementStreamError::UnexpectedMeasurement.into())
            }

            (MeasurementStreamItem::StartGroup(group), State::Started) => {
                self.json.write_key(group)?;
                self.json.write_open_obj();
                self.state = State::WithinGroup;
                Ok(())
            }

            (MeasurementStreamItem::StartGroup(_), _) => {
                Err(MeasurementStreamError::UnexpectedStartOfGroup.into())
            }

            (MeasurementStreamItem::EndGroup, State::WithinGroup) => {
                self.json.write_close_obj();
                self.state = State::Started;
                Ok(())
            }

            (MeasurementStreamItem::EndGroup, _) => {
                Err(MeasurementStreamError::UnexpectedEndOfGroup.into())
            }
        }
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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.end()?;

        let output = serializer.inner().into_string()?;

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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.timestamp(timestamp)?;
        serializer.measurement("temperature", 25.5)?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;
        serializer.measurement("lati", 2300.4)?;
        serializer.end_group()?;
        serializer.measurement("pressure", 255.2)?;
        serializer.end()?;

        let output = serializer.inner().into_string()?;

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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));

        serializer.start()?;
        serializer.end()?;

        let expected_output =
            json!({"type": "ThinEdgeMeasurement", "time": "2021-06-22T17:03:14.123456789+05:00"});

        let output = serializer.inner().into_string()?;

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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.timestamp(timestamp)?;
        serializer.end()?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time":"2021-06-22T17:03:14.123456789+05:00"
        });

        let output = serializer.inner().into_string()?;

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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.start_group("location")?;

        let expected_err = serializer.timestamp(timestamp);

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementStreamError(
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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.end_group();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementStreamError(
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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.start_group("location2");

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementStreamError(
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

        let mut serializer = StreamBuilder::from(C8yJsonSerializer::new(timestamp));
        serializer.start()?;
        serializer.start_group("location")?;
        serializer.measurement("alti", 2100.4)?;
        serializer.measurement("longi", 2200.4)?;

        let expected_err = serializer.inner().into_string();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementStreamError(
                MeasurementStreamError::UnexpectedEndOfData
            ))
        );

        Ok(())
    }
}
