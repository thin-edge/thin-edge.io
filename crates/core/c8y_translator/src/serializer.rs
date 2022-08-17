use std::collections::HashMap;

use thin_edge_json::measurement::MeasurementVisitor;
use time::OffsetDateTime;

pub struct C8yJsonSerializer {
    json: InnerJson,
    in_group: Option<String>,
    default_timestamp: OffsetDateTime,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error(transparent)]
    JsonWriterError(#[from] serde_json::Error),
}

#[derive(Debug, serde::Serialize)]
struct InnerJson {
    #[serde(rename = "type")]
    type_: InnerJsonType,

    #[serde(rename = "externalSource")]
    external_source: Option<ExternalSource>,

    value: Option<C8yJsonValue>,

    #[serde(with = "time::serde::rfc3339::option")]
    time: Option<OffsetDateTime>,

    #[serde(flatten)]
    groups: HashMap<String, HashMap<String, f64>>,

    #[serde(flatten)]
    values: HashMap<String, f64>,
}

#[derive(Debug, serde::Serialize)]
struct C8yJsonValue {
    value: f64,
}

#[derive(Debug, serde::Serialize)]
enum InnerJsonType {
    #[serde(rename = "ThinEdgeMeasurement")]
    ThinEdgeMeasurement,
}

#[derive(Debug, serde::Serialize)]
struct ExternalSource {
    external_id: String,
    type_: ExternalSourceType,
}

#[derive(Debug, serde::Serialize)]
enum ExternalSourceType {
    #[serde(rename = "c8y_Serial")]
    C8YSerial,
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
    pub fn new(default_timestamp: OffsetDateTime, maybe_child_id: Option<&str>) -> Self {
        let mut json = InnerJson {
            type_: InnerJsonType::ThinEdgeMeasurement,
            external_source: None,
            value: None,
            time: None,
            groups: HashMap::new(),
            values: HashMap::new(),
        };

        if let Some(child_id) = maybe_child_id {
            // In case the measurement is addressed to a child-device use fragment
            // "externalSource" to tell c8Y identity API to use child-device
            // object referenced by "externalId", instead of root device object
            // referenced by MQTT client's Device ID.
            json.external_source = Some(ExternalSource {
                external_id: child_id.to_string(),
                type_: ExternalSourceType::C8YSerial,
            });
        }

        Self {
            json,
            in_group: None,
            default_timestamp,
        }
    }

    fn end(&mut self) -> Result<(), C8yJsonSerializationError> {
        if self.json.time.is_none() {
            self.visit_timestamp(self.default_timestamp)?;
        }
        Ok(())
    }

    pub fn into_string(&mut self) -> Result<String, C8yJsonSerializationError> {
        self.end()?;
        Ok(serde_json::to_string(&self.json)?)
    }
}

impl MeasurementVisitor for C8yJsonSerializer {
    type Error = C8yJsonSerializationError;

    fn visit_timestamp(&mut self, timestamp: OffsetDateTime) -> Result<(), Self::Error> {
        self.json.time = Some(timestamp);
        Ok(())
    }

    fn visit_measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        if let Some(group_name) = self.in_group.as_ref() {
            let group = self
                .json
                .groups
                .entry(group_name.to_string())
                .or_insert_with(HashMap::new);
            group.insert(key.to_string(), value);
        } else {
            self.json.values.insert(key.to_string(), value);
        }
        Ok(())
    }

    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.in_group.is_some() {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        self.in_group = Some(group.to_string());
        Ok(())
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        self.in_group = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ::time::macros::datetime;
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;

    use super::*;

    #[test]
    fn serialize_single_value_message() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;
        serializer.visit_measurement("lati", 2300.4)?;
        serializer.visit_end_group()?;
        serializer.visit_measurement("pressure", 255.2)?;

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_timestamp(timestamp)?;

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_start_group("location")?;

        let expected_err = serializer.visit_timestamp(timestamp);

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;

        let expected_err = serializer.visit_end_group();

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;

        let expected_err = serializer.visit_start_group("location2");

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
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, None);
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;

        let expected_err = serializer.into_string();

        assert_matches!(
            expected_err,
            Err(C8yJsonSerializationError::MeasurementCollectorError(
                MeasurementStreamError::UnexpectedEndOfData
            ))
        );

        Ok(())
    }

    #[test]
    fn serialize_timestamp_child_message() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let mut serializer = C8yJsonSerializer::new(timestamp, Some("child1"));
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": "2021-06-22T17:03:14.123456789+05:00",
            "externalSource": {
                "externalId": "child1",
                "type": "c8y_Serial"
            },
            "temperature": {
                "temperature": {
                    "value": 25.5
                }
            }
        });

        let output = serializer.into_string()?;

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(&output)?,
            expected_output
        );

        Ok(())
    }
}
