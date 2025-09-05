//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! # Examples
//!
//! ```
//! use c8y_mapper_ext::json::from_thin_edge_json;
//! use c8y_mapper_ext::entity_cache::CloudEntityMetadata;
//! use tedge_api::entity::EntityMetadata;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
//! let output = from_thin_edge_json(single_value_thin_edge_json, &entity,"",None);
//! ```

use crate::entity_cache::CloudEntityMetadata;
use crate::serializer;
use clock::Clock;
use clock::WallClock;
use std::collections::HashMap;
use tedge_api::measurement::*;
use time::OffsetDateTime;
use time::{self};

#[derive(thiserror::Error, Debug)]
pub enum CumulocityJsonError {
    #[error(transparent)]
    C8yJsonSerializationError(#[from] serializer::C8yJsonSerializationError),

    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] ThinEdgeJsonParserError),
}

/// Converts from thin-edge measurement JSON to C8Y measurement JSON
pub fn from_thin_edge_json(
    input: &str,
    entity: &CloudEntityMetadata,
    m_type: &str,
    units: Option<&Units>,
) -> Result<String, CumulocityJsonError> {
    let timestamp = WallClock.now();
    let c8y_vec = from_thin_edge_json_with_timestamp(input, timestamp, entity, m_type, units)?;
    Ok(c8y_vec)
}

fn from_thin_edge_json_with_timestamp(
    input: &str,
    timestamp: OffsetDateTime,
    entity: &CloudEntityMetadata,
    m_type: &str,
    units: Option<&Units>,
) -> Result<String, CumulocityJsonError> {
    let mut serializer = serializer::C8yJsonSerializer::new(timestamp, entity, m_type, units);
    parse_str(input, &mut serializer)?;
    Ok(serializer.into_string()?)
}

/// Units used for measurements of a given type
#[derive(Default)]
pub struct Units {
    units: HashMap<String, String>,
    group_units: HashMap<String, Units>,
}

impl Units {
    /// An empty set of measurement units
    ///
    /// This is the default when no measurement metadata is published for a measurement topic
    pub fn new() -> Units {
        Units {
            units: HashMap::new(),
            group_units: HashMap::new(),
        }
    }

    /// True if no units are actually defined
    pub fn is_empty(&self) -> bool {
        self.units.is_empty() && self.group_units.is_empty()
    }

    /// Measurement units as defined by metadata published on a measurement topic
    pub fn from_metadata(meta: serde_json::Value) -> Self {
        let mut units = Units::new();
        if let serde_json::Value::Object(map) = meta {
            for (k, v) in map {
                units.set_unit(k, v);
            }
        }
        units
    }

    pub fn set_unit(&mut self, measurement: String, meta: serde_json::Value) {
        if let Some(unit) = meta.get("unit") {
            // "Temperature": {"unit": "°C"},
            if let serde_json::Value::String(unit_name) = unit {
                self.units.insert(measurement, unit_name.to_owned());
            }
        } else {
            // "Climate": { "Temperature": {"unit": "°C"}, "Humidity": {"unit": "%RH"} }
            let group = measurement;
            self.set_group_units(group, meta);
        }
    }

    pub fn set_group_units(&mut self, group: String, meta: serde_json::Value) {
        let units = Units::from_metadata(meta);
        if !units.is_empty() {
            self.group_units.insert(group, units);
        }
    }

    /// Retrieve the unit to be used for a measurement, if any
    pub fn get_unit(&self, measurement: &str) -> Option<&str> {
        self.units.get(measurement).map(|x| x.as_str())
    }

    /// Retrieve the units to be used for a measurement group, if any
    pub fn get_group_units(&self, group: &str) -> Option<&Units> {
        self.group_units.get(group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use proptest::prelude::*;
    use serde_json::json;
    use serde_json::Value;
    use tedge_api::entity::EntityMetadata;
    use test_case::test_case;
    use time::format_description;
    use time::macros::datetime;

    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23.0,
                  "pressure": 220.0
               }"#;

        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json_with_timestamp(
            single_value_thin_edge_json,
            timestamp,
            &entity,
            "",
            None,
        );

        let expected_output = json!({
            "time": timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
            "temperature": {
                "temperature": {
                    "value": 23.0
                }
            },
            "pressure": {
                "pressure": {
                    "value": 220.0
                }
            },
            "type": "ThinEdgeMeasurement"
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn check_type_translation() {
        let single_value_thin_edge_json = r#"{
                  "type": "test",
                  "temperature": 23.0               
               }"#;

        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json_with_timestamp(
            single_value_thin_edge_json,
            timestamp,
            &entity,
            "",
            None,
        );

        let expected_output = json!({
            "time": timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
            "temperature": {
                "temperature": {
                    "value": 23.0
                }
            },
            "type": "test"
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn check_thin_edge_translation_with_timestamp() {
        let single_value_thin_edge_json = r#"{
                  "time" : "2013-06-22T17:03:14.123+02:00",
                  "temperature": 23.0,
                  "pressure": 220.0
               }"#;

        let expected_output = r#"{
                     "time": "2013-06-22T17:03:14.123+02:00",
                     "temperature": {
                         "temperature": {
                               "value": 23.0
                         }
                    },
                    "pressure" : {
                       "pressure": {
                          "value" : 220.0
                          }
                       },
                    "type": "ThinEdgeMeasurement"
                  }"#;

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json(single_value_thin_edge_json, &entity, "", None);

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output.unwrap().split_whitespace().collect::<String>()
        );
    }

    #[test]
    fn check_multi_value_translation() {
        let multi_value_thin_edge_json = r#"{
            "temperature": 25.0 ,
            "location": {
                  "latitude": 32.54,
                  "longitude": -117.67,
                  "altitude": 98.6
              },
            "pressure": 98.0
        }"#;

        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json_with_timestamp(
            multi_value_thin_edge_json,
            timestamp,
            &entity,
            "",
            None,
        );

        let expected_output = json!({
            "time": timestamp
                .format(&format_description::well_known::Rfc3339)
                .unwrap()
                .as_str(),
            "temperature": {
                "temperature": {
                    "value": 25.0
                 }
            },
           "location": {
                "latitude": {
                   "value": 32.54
                 },
                "longitude": {
                  "value": -117.67
                },
                "altitude": {
                  "value": 98.6
               }
          },
         "pressure": {
            "pressure": {
                 "value": 98.0
            }
          },
          "type": "ThinEdgeMeasurement"
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn using_metadata_to_define_measurement_units() {
        let input = r#"
    {
      "time": "2013-06-22T17:03:14.123+02:00",
      "Climate":{
        "Temperature":23.4,
        "Humidity":95.0
      },
      "Acceleration":{
        "X-Axis":0.002,
        "Y-Axis":0.015,
        "Z-Axis":5.0
      }
    }"#;

        let units = r#"
    {
      "Climate":{
        "Temperature": {"unit": "°C"},
        "Humidity": {"unit": "%RH"}
      },
      "Acceleration":{
        "X-Axis": {"unit": "m/s²"},
        "Y-Axis": {"unit": "m/s²"},
        "Z-Axis": {"unit": "m/s²"}
      }
    }"#;

        let expected_output = r#"
    {
      "time": "2013-06-22T17:03:14.123+02:00",
      "Climate": {
        "Temperature": {"value":23.4,"unit":"°C"},
        "Humidity":{"value":95.0,"unit":"%RH"}
      },
      "Acceleration": {
        "X-Axis": {"value":0.002,"unit":"m/s²"},
        "Y-Axis": {"value":0.015,"unit":"m/s²"},
        "Z-Axis": {"value":5.0,"unit":"m/s²"}
      },
      "type": "ThinEdgeMeasurement"
    }"#;

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let units = Units::from_metadata(serde_json::from_str(units).unwrap());
        let output = from_thin_edge_json(input, &entity, "", Some(&units));

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output.unwrap().split_whitespace().collect::<String>()
        );
    }

    #[test]
    fn thin_edge_json_round_tiny_number() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 10e-9999999999
          }"#;

        let expected_output = r#"{
             "time": "2013-06-22T17:03:14+02:00",
             "temperature": {
                 "temperature": {
                    "value": 0.0
                 }
            },
            "type": "ThinEdgeMeasurement"
        }"#;

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json(input, &entity, "", None);

        let actual_output = output.unwrap().split_whitespace().collect::<String>();

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            actual_output
        );
    }

    proptest! {

        #[test]
        fn it_works_for_any_measurement(measurement in r#"[a-z]{3,6}"#) {
            if measurement == "time" || measurement == "type" {
                // Skip this test case, since the random measurement name happens to be a reserved key.
                return Ok(());
            }
            let input = format!(r#"{{"time": "2013-06-22T17:03:14.453+02:00",
                        "{}": 123.0
                      }}"#, measurement);
            let time = "2013-06-22T17:03:14.453+02:00";
            let expected_output = format!(r#"{{
                  "time": "{}",
                  "{}": {{
                  "{}": {{
                       "value": 123.0
                      }}
                   }},
                  "type": "ThinEdgeMeasurement"
                }}"#, time, measurement, measurement);

        let entity = CloudEntityMetadata::new("foo".into(), EntityMetadata::main_device(None));
        let output = from_thin_edge_json(input.as_str(), &entity, "", None).unwrap();
        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output
                .split_whitespace()
                .collect::<String>()
        );
        }
    }

    #[test_case(
    "child1",
    r#"{"temperature": 23.0}"#,
    json!({
        "externalSource": {"externalId": "child1","type": "c8y_Serial",},
        "time": "2021-04-08T00:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}},
        "type": "ThinEdgeMeasurement"
    })
    ;"child device single value thin-edge json translation")]
    #[test_case(
    "child2",
    r#"{"temperature": 23.0, "pressure": 220.0}"#,
    json!({
        "externalSource": {"externalId": "child2","type": "c8y_Serial",},
        "time": "2021-04-08T00:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}},
        "pressure": {"pressure": {"value": 220.0}},
        "type": "ThinEdgeMeasurement"
    })
    ;"child device multiple values thin-edge json translation")]
    #[test_case(
    "child3",
    r#"{"temperature": 23.0, "time": "2021-04-23T19:00:00+05:00"}"#,
    json!({
        "externalSource": {"externalId": "child3","type": "c8y_Serial",},
        "time": "2021-04-23T19:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}},
        "type": "ThinEdgeMeasurement"
    })
    ;"child device single value with timestamp thin-edge json translation")]
    fn check_value_translation_for_child_device(
        child_id: &str,
        thin_edge_json: &str,
        expected_output: Value,
    ) {
        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);
        let entity = CloudEntityMetadata::new(
            child_id.into(),
            EntityMetadata::child_device(child_id.to_string()).unwrap(),
        );
        let output =
            from_thin_edge_json_with_timestamp(thin_edge_json, timestamp, &entity, "", None);
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }
}
