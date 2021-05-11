//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! ```
//! use c8y_translator_lib::json::from_thin_edge_json;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let output = from_thin_edge_json(
//!             &String::from(single_value_thin_edge_json).into_bytes());
//! ```

use crate::serializer;
use chrono::prelude::*;
use thin_edge_json::json::{ThinEdgeJsonParserError, *};

#[derive(thiserror::Error, Debug)]
pub enum CumulocityJsonError {
    #[error(transparent)]
    C8yJsonSerializationError(#[from] serializer::C8yJsonSerializationError),
    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] ThinEdgeJsonParserError<serializer::C8yJsonSerializationError>),
}

/// Convert from thin-edge Json to c8y_json
pub fn from_thin_edge_json(input: &[u8]) -> Result<Vec<u8>, serializer::C8yJsonSerializationError> {
    let timestamp = thin_edge_json::measurement::current_timestamp();
    let c8y_vec = from_thin_edge_json_with_timestamp(input, timestamp)?;
    Ok(c8y_vec)
}

fn from_thin_edge_json_with_timestamp(
    input: &[u8],
    default_timestamp: DateTime<FixedOffset>,
) -> Result<Vec<u8>, serializer::C8yJsonSerializationError> {
    let mut serializer = serializer::C8yJsonSerializer::new(default_timestamp)?;
    let _ = parse_utf8(input, &mut serializer).map_err(ThinEdgeJsonParserError::VisitorError);
    serializer.bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use serde_json::json;

    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = br#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let timestamp = FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0);

        let output = from_thin_edge_json_with_timestamp(single_value_thin_edge_json, timestamp);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
            "temperature": {
                "temperature": {
                    "value": 23
                }
            },
            "pressure": {
                "pressure": {
                    "value": 220
                }
            }
        });

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
            expected_output
        );
    }

    #[test]
    fn check_thin_edge_translation_with_timestamp() {
        let single_value_thin_edge_json = r#"{
                  "time" : "2013-06-22T17:03:14.123+02:00",
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let expected_output = r#"{
                     "type": "ThinEdgeMeasurement",
                     "time": "2013-06-22T17:03:14.123+02:00",
                     "temperature": {
                         "temperature": {
                               "value": 23
                         }
                    },
                    "pressure" : {
                       "pressure": {
                          "value" : 220
                          }
                       }
                  }"#;

        let output = from_thin_edge_json(&String::from(single_value_thin_edge_json).into_bytes());

        let vec = output.unwrap();
        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            String::from_utf8(vec)
                .unwrap()
                .split_whitespace()
                .collect::<String>()
        );
    }

    #[test]
    fn check_multi_value_translation() {
        let multi_value_thin_edge_json = br#"{
            "temperature": 25 ,
            "location": {
                  "latitude": 32.54,
                  "longitude": -117.67,
                  "altitude": 98.6
              },
            "pressure": 98
        }"#;

        let timestamp = FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0);

        let output = from_thin_edge_json_with_timestamp(multi_value_thin_edge_json, timestamp);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
            "temperature": {
                "temperature": {
                    "value": 25
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
                 "value": 98
            }
          }
        });

        assert_json_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.unwrap()).unwrap(),
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
                    "value": 0
                 }
            }
        }"#;

        let output = from_thin_edge_json(&String::from(input).into_bytes());

        let actual_output = String::from_utf8(output.unwrap())
            .unwrap()
            .split_whitespace()
            .collect::<String>();

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            actual_output
        );
    }
    use proptest::prelude::*;

    proptest! {

        #[test]
        fn it_works_for_any_measurement(measurement in r#"[a-z]{3,6}"#) {
            if measurement == "time" || measurement == "type" {
                // Skip this test case, since the random measurement name happens to be a reserved key.
                return Ok(());
            }
            let input = format!(r#"{{"time": "2013-06-22T17:03:14.453+02:00",
                        "{}": 123
                      }}"#, measurement);
            let time = "2013-06-22T17:03:14.453+02:00";
            let expected_output = format!(r#"{{
                  "type": "ThinEdgeMeasurement",
                  "time": "{}",
                  "{}": {{
                  "{}": {{
                       "value": 123
                      }}
                   }}
                }}"#, time, measurement, measurement);

        let output = from_thin_edge_json(&input.into_bytes()).unwrap();
        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            String::from_utf8(output)
                .unwrap()
                .split_whitespace()
                .collect::<String>()
        );
        }
    }
}
