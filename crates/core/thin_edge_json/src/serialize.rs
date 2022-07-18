use std::collections::HashMap;

use crate::measurement::MeasurementVisitor;
use time::OffsetDateTime;

pub struct ThinEdgeJsonSerializer {
    json: InnerJson,
    within_group: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct InnerJson {
    #[serde(with = "time::serde::rfc3339::option")]
    #[serde(rename = "time")]
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<OffsetDateTime>,

    #[serde(flatten)]
    values: HashMap<String, f64>,

    #[serde(flatten)]
    groups: HashMap<String, HashMap<String, f64>>,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonSerializationError {
    #[error(transparent)]
    FormatError(#[from] std::fmt::Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),

    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error("Serializer produced invalid Utf8 string")]
    InvalidUtf8ConversionToString(std::string::FromUtf8Error),

    #[error(transparent)]
    JsonWriterError(#[from] serde_json::Error),
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

    pub fn new_with_timestamp(default_timestamp: Option<OffsetDateTime>) -> Self {
        Self {
            json: InnerJson {
                timestamp: default_timestamp,
                groups: HashMap::new(),
                values: HashMap::new(),
            },
            within_group: None,
        }
    }

    fn end(&mut self) -> Result<(), ThinEdgeJsonSerializationError> {
        if self.within_group.is_some() {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }
        Ok(())
    }

    pub fn bytes(mut self) -> Result<Vec<u8>, ThinEdgeJsonSerializationError> {
        self.into_string().map(String::into_bytes)
    }

    pub fn into_string(&mut self) -> Result<String, ThinEdgeJsonSerializationError> {
        serde_json::to_string(&self.json).map_err(ThinEdgeJsonSerializationError::from)
    }
}

impl Default for ThinEdgeJsonSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurementVisitor for ThinEdgeJsonSerializer {
    type Error = ThinEdgeJsonSerializationError;

    fn visit_timestamp(&mut self, timestamp: OffsetDateTime) -> Result<(), Self::Error> {
        if self.within_group.is_some() {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        self.json.timestamp = Some(timestamp);
        Ok(())
    }

    fn visit_measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if let Some(group_name) = self.within_group.as_ref() {
            let group = self
                .json
                .groups
                .entry(group_name.to_string())
                .or_insert_with(HashMap::new);
            group.insert(name.to_string(), value);
        } else {
            self.json.values.insert(name.to_string(), value);
        }
        Ok(())
    }

    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.within_group.is_some() {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        self.within_group = Some(group.to_string());
        Ok(())
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        if self.within_group.is_none() {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.within_group = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use time::format_description;

    fn test_timestamp() -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    #[test]
    fn serialize_single_value_message() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();

        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;

        let body = r#""temperature":25.5"#;
        let expected_output = format!(
            r#"{{"time":"{}",{}}}"#,
            timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
            body
        );
        let output = serializer.into_string()?;
        assert_eq!(output, expected_output);
        Ok(())
    }

    #[test]
    fn serialize_single_value_no_timestamp_message() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.visit_measurement("temperature", 25.5)?;
        let expected_output = r#"{"temperature":25.5}"#;
        let output = serializer.into_string()?;
        assert_eq!(output, expected_output);
        Ok(())
    }

    #[test]
    fn serialize_multi_value_message() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;
        serializer.visit_measurement("lati", 2300.4)?;
        serializer.visit_end_group()?;
        serializer.visit_measurement("pressure", 255.0)?;

        assert!(serializer.json.timestamp.is_some());
        assert_eq!(serializer.json.timestamp.clone().unwrap(), timestamp);

        // We cannot reliably compare float values for equality, so this must suffice:
        assert!(serializer.json.values.get("temperature").is_some());

        // We cannot reliably compare float values for equality, so this must suffice:
        assert!(serializer.json.values.get("pressure").is_some());

        assert!(serializer.json.groups.get("location").is_some());
        assert!(serializer
            .json
            .groups
            .get("location")
            .unwrap()
            .get("alti")
            .is_some());
        assert!(serializer
            .json
            .groups
            .get("location")
            .unwrap()
            .get("longi")
            .is_some());
        assert!(serializer
            .json
            .groups
            .get("location")
            .unwrap()
            .get("lati")
            .is_some());

        Ok(())
    }

    #[test]
    fn serialize_empty_message() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let expected_output = "{}";
        let output = serializer.into_string()?;
        assert_eq!(expected_output, output);
        Ok(())
    }

    #[test]
    fn serialize_timestamp_message() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.visit_timestamp(timestamp)?;
        let expected_output = format!(
            r#"{{"time":"{}"{}"#,
            timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
            "}"
        );
        let output = serializer.into_string()?;
        assert_eq!(expected_output, output);
        Ok(())
    }

    #[test]
    fn serialize_timestamp_within_group() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        let timestamp = test_timestamp();
        serializer.visit_start_group("location")?;
        let result = serializer.visit_timestamp(timestamp);
        let expected_error = "Unexpected time stamp within a group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }

    #[test]
    fn serialize_unexpected_end_of_group() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;
        let result = serializer.visit_end_group();
        let expected_error = "Unexpected end of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }

    #[test]
    fn serialize_unexpected_start_of_group() -> anyhow::Result<()> {
        let mut serializer = ThinEdgeJsonSerializer::new();
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;
        let result = serializer.visit_start_group("location");
        let expected_error = "Unexpected start of group";
        assert_eq!(expected_error, result.unwrap_err().to_string());
        Ok(())
    }
}
