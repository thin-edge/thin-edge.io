use crate::entity_cache::CloudEntityMetadata;
use crate::json::Units;
use json_writer::JsonWriter;
use json_writer::JsonWriterError;
use tedge_api::entity::EntityType;
use tedge_api::measurement::MeasurementVisitor;
use time::format_description;
use time::OffsetDateTime;

pub struct C8yJsonSerializer<'a> {
    json: JsonWriter,
    is_within_group: bool,
    timestamp_present: bool,
    default_timestamp: OffsetDateTime,
    type_present: bool,
    default_type: String,
    units: Option<&'a Units>,
    group_units: Option<&'a Units>,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonSerializationError {
    #[error(transparent)]
    MeasurementCollectorError(#[from] MeasurementStreamError),

    #[error(transparent)]
    JsonWriterError(#[from] JsonWriterError),

    #[error("Unexpected measurement name: \"{name}\" is a reserved word.")]
    UnexpectedMeasurementName { name: String },
}

#[allow(clippy::enum_variant_names)]
#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum MeasurementStreamError {
    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("Unexpected type within a group")]
    UnexpectedType,

    #[error("Unexpected end of data")]
    UnexpectedEndOfData,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,
}

impl<'a> C8yJsonSerializer<'a> {
    pub fn new(
        default_timestamp: OffsetDateTime,
        entity: &CloudEntityMetadata,
        m_type: &str,
        units: Option<&'a Units>,
    ) -> Self {
        let capa = 1024; // XXX: Choose a capacity based on expected JSON length.
        let mut json = JsonWriter::with_capacity(capa);
        let default_type = if m_type.is_empty() {
            "ThinEdgeMeasurement".to_owned()
        } else {
            m_type.into()
        };

        json.write_open_obj();

        if entity.metadata.r#type == EntityType::ChildDevice
            || entity.metadata.r#type == EntityType::Service
        {
            let entity_id = &entity.external_id;
            // In case the measurement is addressed to a child-device or a service, use fragment
            // "externalSource" to tell c8Y identity API to use child-device or for service
            // object referenced by "externalId", instead of root device object
            // referenced by MQTT client's Device ID.
            let _ = json.write_key("externalSource");
            json.write_open_obj();
            let _ = json.write_key("externalId");
            let _ = json.write_str(entity_id.as_ref());
            let _ = json.write_key("type");
            let _ = json.write_str("c8y_Serial");
            json.write_close_obj();
        }

        Self {
            json,
            is_within_group: false,
            timestamp_present: false,
            default_timestamp,
            type_present: false,
            default_type,
            units,
            group_units: None,
        }
    }

    fn end(&mut self) -> Result<(), C8yJsonSerializationError> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfData.into());
        }

        if !self.timestamp_present {
            self.visit_timestamp(self.default_timestamp)?;
        }

        if !self.type_present {
            self.visit_text_property("type", self.default_type.to_owned().as_str())?;
        }

        assert!(self.timestamp_present);

        self.json.write_close_obj();
        Ok(())
    }

    fn write_value_obj(
        &mut self,
        value: f64,
        unit: Option<&str>,
    ) -> Result<(), C8yJsonSerializationError> {
        self.json.write_open_obj();
        self.json.write_key("value")?;
        self.json.write_f64(value)?;
        if let Some(unit) = unit {
            self.json.write_key("unit")?;
            self.json.write_str(unit)?;
        }
        self.json.write_close_obj();
        Ok(())
    }

    pub fn into_string(mut self) -> Result<String, C8yJsonSerializationError> {
        self.end()?;
        Ok(self.json.clone().into_string()?)
    }
}

impl MeasurementVisitor for C8yJsonSerializer<'_> {
    type Error = C8yJsonSerializationError;

    fn visit_timestamp(&mut self, timestamp: OffsetDateTime) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedTimestamp.into());
        }

        self.json.write_key("time")?;
        self.json.write_str(
            timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
        )?;

        self.timestamp_present = true;
        Ok(())
    }

    fn visit_text_property(&mut self, name: &str, value: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedType.into());
        }
        match name {
            "type" => {
                self.json.write_key("type")?;
                self.json.write_str(value)?;

                self.type_present = true;
            }

            "externalSource" => {
                return Err(C8yJsonSerializationError::UnexpectedMeasurementName {
                    name: name.to_string(),
                });
            }

            _ => {}
        }
        Ok(())
    }

    fn visit_measurement(&mut self, key: &str, value: f64) -> Result<(), Self::Error> {
        match key {
            "type" => {
                return Err(C8yJsonSerializationError::UnexpectedMeasurementName {
                    name: key.to_string(),
                });
            }
            "externalSource" => {
                return Err(C8yJsonSerializationError::UnexpectedMeasurementName {
                    name: key.to_string(),
                });
            }
            _ => {
                self.json.write_key(key)?;

                if self.is_within_group {
                    let unit = self.group_units.and_then(|units| units.get_unit(key));
                    self.write_value_obj(value, unit)?;
                } else {
                    let unit = self.units.and_then(|units| units.get_unit(key));
                    self.json.write_open_obj();
                    self.json.write_key(key)?;
                    self.write_value_obj(value, unit)?;
                    self.json.write_close_obj();
                }
            }
        }
        Ok(())
    }

    fn visit_json_property(
        &mut self,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), Self::Error> {
        if key == "properties" && value.is_object() {
            if let Some(properties) = value.as_object() {
                for (k, v) in properties {
                    self.json.write_key(k)?;
                    self.json.write_value(v)?;
                }
            }
        }

        Ok(())
    }

    fn visit_start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedStartOfGroup.into());
        }

        self.group_units = self.units.and_then(|units| units.get_group_units(group));
        self.json.write_key(group)?;
        self.json.write_open_obj();
        self.is_within_group = true;
        Ok(())
    }

    fn visit_end_group(&mut self) -> Result<(), Self::Error> {
        if !self.is_within_group {
            return Err(MeasurementStreamError::UnexpectedEndOfGroup.into());
        }

        self.json.write_close_obj();
        self.is_within_group = false;
        self.group_units = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ::time::macros::datetime;
    use assert_json_diff::*;
    use assert_matches::*;
    use serde_json::json;
    use tedge_api::entity::EntityMetadata;

    use super::*;

    #[test]
    fn serialize_single_value_message() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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
    fn serialize_single_value_message_with_custom_type() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "test_type", None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;

        let output = serializer.into_string()?;

        let expected_output = json!({
            "type": "test_type",
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
    fn invalid_to_have_type_as_measurement() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        let res = serializer.visit_measurement("type", 1234.0).unwrap_err();

        let expected_output = r#"Unexpected measurement name: "type" is a reserved word."#;

        assert_json_eq!(res.to_string(), expected_output);
        Ok(())
    }

    #[test]
    fn invalid_to_have_externalsource_as_measurement() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        let res = serializer
            .visit_measurement("externalSource", 1234.0)
            .unwrap_err();

        let expected_output =
            r#"Unexpected measurement name: "externalSource" is a reserved word."#;

        assert_json_eq!(res.to_string(), expected_output);
        Ok(())
    }

    #[test]
    fn serialize_multi_value_message() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));

        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        serializer.visit_start_group("location")?;
        serializer.visit_measurement("alti", 2100.4)?;
        serializer.visit_measurement("longi", 2200.4)?;
        serializer.visit_measurement("lati", 2300.4)?;
        serializer.visit_end_group()?;
        serializer.visit_measurement("pressure", 255.2)?;
        serializer.visit_text_property("type", "TestMeasurement")?;
        let output = serializer.into_string()?;

        let expected_output = json!({
            "type": "TestMeasurement",
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
    fn type_inside_a_group_is_not_valid() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
        serializer.visit_timestamp(timestamp)?;
        serializer.visit_measurement("temperature", 25.5)?;
        serializer.visit_start_group("location")?;
        let err = serializer
            .visit_text_property("type", "TestMeasurement")
            .unwrap_err();

        assert_eq!(err.to_string(), "Unexpected type within a group");

        Ok(())
    }

    #[test]
    fn serialize_empty_message() -> anyhow::Result<()> {
        let timestamp = datetime!(2021-06-22 17:03:14.123456789 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);

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

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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

        let entity = CloudEntityMetadata::new(
            "child1".into(),
            EntityMetadata::child_device("child1".to_string())?,
        );
        let mut serializer = C8yJsonSerializer::new(timestamp, &entity, "", None);
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
