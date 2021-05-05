//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! ```
//! use c8y_translator_lib::CumulocityJson;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let output = CumulocityJson::from_thin_edge_json(
//!             &String::from(single_value_thin_edge_json));
//! ```

use crate::serializer;
use chrono::prelude::*;
use thin_edge_json::{
    json::{ThinEdgeJson, ThinEdgeValue},
    measurement::GroupedMeasurementVisitor,
};

#[derive(Debug, Eq, PartialEq)]
pub struct CumulocityJson;

impl CumulocityJson {
    ///Convert from thinedgejson to c8y_json
    pub fn from_thin_edge_json(
        input: &[u8],
    ) -> Result<Vec<u8>, serializer::C8yJsonSerializationError> {
        let local_time_now: DateTime<Local> = Local::now();
        let timestamp = local_time_now.with_timezone(local_time_now.offset());
        let c8y_vec = Self::from_thin_edge_json_with_timestamp(input, timestamp)?;
        Ok(c8y_vec)
    }

    fn from_thin_edge_json_with_timestamp(
        input: &[u8],
        timestamp: DateTime<FixedOffset>,
    ) -> Result<Vec<u8>, serializer::C8yJsonSerializationError> {
        let measurements = ThinEdgeJson::from_utf8(input, timestamp)?;

        let mut serializer = serializer::C8yJsonSerializer::new()?;
        serializer.timestamp(measurements.timestamp)?;

        for v in measurements.values.iter() {
            match v {
                ThinEdgeValue::Single(measurement) => {
                    serializer.measurement(&measurement.name, measurement.value)?;
                }
                ThinEdgeValue::Multi(measurement) => {
                    serializer.start_group(&measurement.name)?;
                    for s in measurement.values.iter() {
                        serializer.measurement(&s.name, s.value)?;
                    }
                    serializer.end_group()?;
                }
            }
        }
        Ok(serializer.bytes()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_timestamp() -> DateTime<FixedOffset> {
        FixedOffset::east(5 * 3600)
            .ymd(2021, 04, 08)
            .and_hms(0, 0, 0)
    }
    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let type_string = "{\"type\": \"ThinEdgeMeasurement\",";

        let body_of_message = "\"temperature\": {
               \"temperature\": {
                       \"value\": 23
                       }
              },
              \"pressure\": {
                  \"pressure\": {
                      \"value\": 220
                  }
              }
         }";

        let expected_output = format!(
            "{} \"time\":\"{}\",{}",
            type_string,
            test_timestamp().to_rfc3339(),
            body_of_message
        );

        let output = CumulocityJson::from_thin_edge_json_with_timestamp(
            &String::from(single_value_thin_edge_json).into_bytes(),
            test_timestamp(),
        );
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

        let output = CumulocityJson::from_thin_edge_json(
            &String::from(single_value_thin_edge_json).into_bytes(),
        );

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
        let utc_time_now: DateTime<Utc> = Utc::now();
        let type_string = "{\"type\": \"ThinEdgeMeasurement\",";

        let input = r#"{
                "temperature": 25 ,
                "location": {
                      "latitude": 32.54,
                      "longitude": -117.67,
                      "altitude": 98.6
                  },
                "pressure": 98
        }"#;

        let body_of_message = "

            \"temperature\": {
                \"temperature\": {
                    \"value\": 25
                 }
            },
           \"location\": {
                \"latitude\": {
                   \"value\": 32.54
                 },
                \"longitude\": {
                  \"value\": -117.67
                },
                \"altitude\": {
                  \"value\": 98.6
               }
          },
         \"pressure\": {
            \"pressure\": {
                 \"value\": 98
            }
          }
        }";

        let expected_output = format!(
            "{} \"time\":\"{}\",{}",
            type_string,
            utc_time_now.to_rfc3339(),
            body_of_message
        );

        let output = CumulocityJson::from_thin_edge_json_with_timestamp(
            &String::from(input).into_bytes(),
            DateTime::parse_from_rfc3339(&utc_time_now.to_rfc3339()).unwrap(),
        );
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

        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

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

        let output = CumulocityJson::from_thin_edge_json(&input.into_bytes()).unwrap();
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
