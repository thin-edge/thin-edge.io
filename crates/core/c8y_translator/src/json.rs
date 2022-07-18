//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! # Examples
//!
//! ```
//! use c8y_translator::json::from_thin_edge_json;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let output = from_thin_edge_json(single_value_thin_edge_json);
//! ```

use crate::serializer;
use clock::{Clock, WallClock};
use json_writer::{JsonWriter, JsonWriterError};
use thin_edge_json::{event::ThinEdgeEventData, parser::*};
use time::{self, OffsetDateTime};

#[derive(thiserror::Error, Debug)]
pub enum CumulocityJsonError {
    #[error(transparent)]
    C8yJsonSerializationError(#[from] serializer::C8yJsonSerializationError),

    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] ThinEdgeJsonParserError),

    #[error(transparent)]
    JsonWriterError(#[from] JsonWriterError),
}

/// Converts from thin-edge measurement JSON to C8Y measurement JSON
pub fn from_thin_edge_json(input: &str) -> Result<String, CumulocityJsonError> {
    let timestamp = WallClock.now();
    let c8y_vec = from_thin_edge_json_with_timestamp(input, timestamp, None)?;
    Ok(c8y_vec)
}

/// Converts from thin-edge Json to c8y_json with child id information
pub fn from_thin_edge_json_with_child(
    input: &str,
    child_id: &str,
) -> Result<String, CumulocityJsonError> {
    let timestamp = WallClock.now();
    let c8y_vec = from_thin_edge_json_with_timestamp(input, timestamp, Some(child_id))?;
    Ok(c8y_vec)
}

fn from_thin_edge_json_with_timestamp(
    input: &str,
    timestamp: OffsetDateTime,
    maybe_child_id: Option<&str>,
) -> Result<String, CumulocityJsonError> {
    let mut serializer = serializer::C8yJsonSerializer::new(timestamp, maybe_child_id);
    let () = parse_str(input, &mut serializer)?;
    Ok(serializer.into_string()?)
}

pub fn from_thin_edge_json_child_event(
    c_id: &str,
    event_data: &mut Option<ThinEdgeEventData>,
) -> Result<(), CumulocityJsonError> {
    let mut json = JsonWriter::with_capacity(1024);
    let _ = json.write_open_obj();
    let _ = json.write_key("externalId");
    let _ = json.write_str(c_id);
    let _ = json.write_key("type");
    let _ = json.write_str("c8y_Serial");
    let _ = json.write_close_obj();
    event_data.as_mut().map(|e: &mut ThinEdgeEventData| {
        e.extras.insert(
            "externalSource".into(),
            serde_json::from_str(&json.into_string().ok()?).ok()?,
        )
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use proptest::prelude::*;
    use serde_json::{json, Value};
    use test_case::test_case;
    use time::{format_description, macros::datetime};

    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23.0,
                  "pressure": 220.0
               }"#;

        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);

        let output =
            from_thin_edge_json_with_timestamp(single_value_thin_edge_json, timestamp, None);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
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
            }
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
                     "type": "ThinEdgeMeasurement",
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
                       }
                  }"#;

        let output = from_thin_edge_json(single_value_thin_edge_json);

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

        let output =
            from_thin_edge_json_with_timestamp(multi_value_thin_edge_json, timestamp, None);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
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
          }
        });

        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn thin_edge_json_round_tiny_number() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 10e-9999999999
          }"#;

        let expected_output = r#"{
             "type": "ThinEdgeMeasurement",
             "time": "2013-06-22T17:03:14+02:00",
             "temperature": {
                 "temperature": {
                    "value": 0.0
                 }
            }
        }"#;

        let output = from_thin_edge_json(input);

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
                  "type": "ThinEdgeMeasurement",
                  "time": "{}",
                  "{}": {{
                  "{}": {{
                       "value": 123.0
                      }}
                   }}
                }}"#, time, measurement, measurement);

        let output = from_thin_edge_json(input.as_str()).unwrap();
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
        "type": "ThinEdgeMeasurement",
        "externalSource": {"externalId": "child1","type": "c8y_Serial",},
        "time": "2021-04-08T00:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}}
    })
    ;"child device single value thin-edge json translation")]
    #[test_case(
    "child2",
    r#"{"temperature": 23.0, "pressure": 220.0}"#,
    json!({
        "type": "ThinEdgeMeasurement",
        "externalSource": {"externalId": "child2","type": "c8y_Serial",},
        "time": "2021-04-08T00:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}},
        "pressure": {"pressure": {"value": 220.0}}
    })
    ;"child device multiple values thin-edge json translation")]
    #[test_case(
    "child3",
    r#"{"temperature": 23.0, "time": "2021-04-23T19:00:00+05:00"}"#,
    json!({
        "type": "ThinEdgeMeasurement",
        "externalSource": {"externalId": "child3","type": "c8y_Serial",},
        "time": "2021-04-23T19:00:00+05:00",
        "temperature": {"temperature": {"value": 23.0}},
    })
    ;"child device single value with timestamp thin-edge json translation")]
    fn check_value_translation_for_child_device(
        child_id: &str,
        thin_edge_json: &str,
        expected_output: Value,
    ) {
        let timestamp = datetime!(2021-04-08 0:00:0 +05:00);
        let output = from_thin_edge_json_with_timestamp(thin_edge_json, timestamp, Some(child_id));
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output.unwrap().as_str()).unwrap(),
            expected_output
        );
    }
}
