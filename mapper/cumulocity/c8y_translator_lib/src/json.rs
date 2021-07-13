//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! # Examples
//!
//! ```
//! use c8y_translator_lib::json::from_thin_edge_json;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let output = from_thin_edge_json(single_value_thin_edge_json);
//! ```

use crate::serializer;
use chrono::prelude::*;
use clock::{Clock, WallClock};
use thin_edge_json::parser::*;

#[derive(thiserror::Error, Debug)]
pub enum CumulocityJsonError {
    #[error(transparent)]
    C8yJsonSerializationError(#[from] serializer::C8yJsonSerializationError),

    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] ThinEdgeJsonParserError),
}

/// Converts from thin-edge Json to c8y_json
pub fn from_thin_edge_json(input: &str) -> Result<String, CumulocityJsonError> {
    let timestamp = WallClock.now();
    let c8y_vec = from_thin_edge_json_with_timestamp(input, timestamp)?;
    Ok(c8y_vec)
}

fn from_thin_edge_json_with_timestamp(
    input: &str,
    default_timestamp: DateTime<FixedOffset>,
) -> Result<String, CumulocityJsonError> {
    let mut serializer = serializer::C8yJsonSerializer::new(default_timestamp);
    let () = parse_str(input, &mut serializer)?;
    Ok(serializer.into_string()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::*;
    use serde_json::json;

    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23.0,
                  "pressure": 220.0
               }"#;

        let timestamp = FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0);

        let output = from_thin_edge_json_with_timestamp(single_value_thin_edge_json, timestamp);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
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

        let timestamp = FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0);

        let output = from_thin_edge_json_with_timestamp(multi_value_thin_edge_json, timestamp);

        let expected_output = json!({
            "type": "ThinEdgeMeasurement",
            "time": timestamp.to_rfc3339(),
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
    use proptest::prelude::*;

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
}
