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
//!             &String::from(single_value_thin_edge_json).into_bytes());
//! ```

use chrono::format::ParseError;
use chrono::prelude::*;
use json::JsonValue;
use std::fmt;

/// ThinEdgeJson is represented in this struct
/// Since json does not understand DateTime format, the time stamp is represented as a string
/// Before populating the struct members the thinedge json values and names will be validated

pub struct ThinEdgeJson {
    timestamp: String,
    values: Vec<ThinEdgeValue>,
}

enum ThinEdgeValue {
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}

pub struct SingleValueMeasurement {
    name: String,
    value: f64,
}

pub struct MultiValueMeasurement {
    name: String,
    values: Vec<SingleValueMeasurement>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct CumulocityJson {
    c8y_json: JsonValue,
}

impl ThinEdgeJson {
    pub fn from_utf8(
        input: &[u8],
        timestamp: DateTime<Utc>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let json_string = std::str::from_utf8(input)
            .map_err(|err| ThinEdgeJsonError::new_invalid_utf8(input, err))?;
        ThinEdgeJson::from_str(json_string, timestamp)
    }

    pub fn from_str(
        json_string: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        match json::parse(&json_string) {
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj, timestamp),
            Err(err) => Err(ThinEdgeJsonError::new_invalid_json(json_string, err)),
        }
    }

    ///Confirms that the json is in thin-edge json format or not
    fn from_json(
        input: json::JsonValue,
        timestamp: DateTime<Utc>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let mut measurements = vec![];
        let mut time = timestamp.to_rfc3339();
        match &input {
            JsonValue::Object(thin_edge_obj) => {
                for (k, v) in thin_edge_obj.iter() {
                    if k.eq("type") {
                        return Err(ThinEdgeJsonError::ThinEdgeReservedWordError {
                            name: String::from(k),
                        });
                    } else if k.eq("time") {
                        time = ThinEdgeJson::check_timestamp_for_iso8601_complaint(v)?;
                    } else {
                        match v {
                            //Single Value object
                            JsonValue::Number(num) => {
                                let single_value_measurement =
                                    SingleValueMeasurement::new(k, (*num).into())?;
                                measurements.push(ThinEdgeValue::Single(single_value_measurement));
                            }
                            //Multi value object
                            JsonValue::Object(multi_value_thin_edge_object) => {
                                let multi_value_measurement =
                                    MultiValueMeasurement::new(k, multi_value_thin_edge_object)?;
                                measurements.push(ThinEdgeValue::Multi(multi_value_measurement));
                            }

                            _ => {
                                return Err(ThinEdgeJsonError::new_invalid_json_value(k, v));
                            }
                        }
                    }
                }
                Ok(ThinEdgeJson {
                    timestamp: time,
                    values: measurements,
                })
            }
            _ => Err(ThinEdgeJsonError::new_invalid_json_root(&input)),
        }
    }

    fn check_timestamp_for_iso8601_complaint(
        value: &JsonValue,
    ) -> Result<String, ThinEdgeJsonError> {
        match value {
            JsonValue::Short(str) => {
                let timestamp = str.as_str();
                //Parse fails if timestamp is not is8601 complaint
                DateTime::parse_from_rfc3339(&timestamp).map_err(|err| {
                    ThinEdgeJsonError::InvalidTimestamp {
                        value: String::from(timestamp),
                        from: err,
                    }
                })?;
                Ok(String::from(timestamp))
            }
            _ => Err(ThinEdgeJsonError::new_invalid_json_time(value)),
        }
    }
}

impl SingleValueMeasurement {
    fn new(name: &str, value: f64) -> Result<Self, ThinEdgeJsonError> {
        if value == 0.0 || value.is_normal() {
            let single_value = SingleValueMeasurement {
                name: String::from(name),
                value,
            };
            Ok(single_value)
        } else {
            Err(ThinEdgeJsonError::InvalidThinEdgeJsonNumber {
                name: String::from(name),
            })
        }
    }
}

impl MultiValueMeasurement {
    fn new(name: &str, multi_value_obj: &json::object::Object) -> Result<Self, ThinEdgeJsonError> {
        let mut single_values = vec![];

        for (k, v) in multi_value_obj.iter() {
            match v {
                JsonValue::Number(num) => {
                    //Single Value object
                    let single_value_measurement = SingleValueMeasurement::new(k, (*num).into())?;
                    single_values.push(single_value_measurement);
                }
                JsonValue::Object(_object) => {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeHierarchy {
                        name: String::from(k),
                    })
                }
                value => {
                    return Err(ThinEdgeJsonError::new_invalid_json_value(name, value));
                }
            }
        }
        Ok(MultiValueMeasurement {
            name: String::from(name),
            values: single_values,
        })
    }
}

impl CumulocityJson {
    fn new(timestamp: &str, c8y_msg_type: &str) -> CumulocityJson {
        let json_object: JsonValue = JsonValue::new_object();
        let mut c8y_object: CumulocityJson = CumulocityJson {
            c8y_json: json_object,
        };
        c8y_object.c8y_json = JsonValue::new_object();
        c8y_object.insert_into_json_object("type", c8y_msg_type.into());
        c8y_object.insert_into_json_object("time", timestamp.into());

        c8y_object
    }

    ///Convert from thinedgejson to c8y_json
    pub fn from_thin_edge_json(input: &[u8]) -> Result<Vec<u8>, ThinEdgeJsonError> {
        Ok(Self::from_thin_edge_json_with_timestamp(input, Utc::now())?)
    }

    fn from_thin_edge_json_with_timestamp(
        input: &[u8],
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<u8>, ThinEdgeJsonError> {
        let measurements = ThinEdgeJson::from_utf8(input, timestamp)?;
        let mut c8y_object = CumulocityJson::new(&measurements.timestamp, "ThinEdgeMeasurement");
        for v in measurements.values.iter() {
            match v {
                ThinEdgeValue::Single(thin_edge_single_value_measurement) => {
                    c8y_object.translate_into_c8y_single_value_object(
                        &thin_edge_single_value_measurement,
                    );
                }
                ThinEdgeValue::Multi(thin_edge_multi_value_measurement) => {
                    c8y_object
                        .translate_into_c8y_multi_value_object(&thin_edge_multi_value_measurement);
                }
            }
        }
        Ok(c8y_object.deserialize_c8y_json())
    }

    fn translate_into_c8y_single_value_object(&mut self, single: &SingleValueMeasurement) {
        let single_value_object: JsonValue = JsonValue::new_object();
        let mut single_value_c8y_object: CumulocityJson = CumulocityJson {
            c8y_json: single_value_object,
        };
        single_value_c8y_object.insert_into_json_object(
            &single.name,
            CumulocityJson::create_value_object(single.value.into()),
        );

        self.insert_into_json_object(&single.name, single_value_c8y_object.c8y_json);
    }

    fn translate_into_c8y_multi_value_object(&mut self, multi: &MultiValueMeasurement) {
        let multi_value_object: JsonValue = JsonValue::new_object();
        let mut multi_value_c8y_object: CumulocityJson = CumulocityJson {
            c8y_json: multi_value_object,
        };
        for s in multi.values.iter() {
            multi_value_c8y_object.insert_into_json_object(&s.name, s.value.into());
            multi_value_c8y_object.insert_into_json_object(
                &s.name,
                CumulocityJson::create_value_object(s.value.into()),
            );
        }
        self.insert_into_json_object(&multi.name, multi_value_c8y_object.c8y_json);
    }

    ///We are sure that the insert call will not fail and panic
    fn insert_into_json_object(&mut self, name: &str, value: JsonValue) {
        self.c8y_json.insert(name, value).unwrap();
    }

    fn create_value_object(value: JsonValue) -> JsonValue {
        let json_object = JsonValue::new_object();
        let mut value_object: CumulocityJson = CumulocityJson {
            c8y_json: json_object,
        };

        value_object.insert_into_json_object("value", value);
        value_object.c8y_json
    }

    pub fn deserialize_c8y_json(&mut self) -> Vec<u8> {
        self.c8y_json.to_string().into_bytes()
    }
}

impl fmt::Display for CumulocityJson {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.c8y_json)
    }
}

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum ThinEdgeJsonError {
    #[error("Invalid UTF8: {from}: {input_excerpt}...")]
    InvalidUTF8 {
        input_excerpt: String,
        from: std::str::Utf8Error,
    },

    #[error("Invalid JSON: {from}: {input_excerpt}")]
    InvalidJson {
        input_excerpt: String,
        from: json::Error,
    },

    #[error("Invalid Thin Edge measurement: it cannot be {actual_type}: {json_excerpt}")]
    InvalidThinEdgeJsonRoot {
        json_excerpt: String,
        actual_type: String,
    },

    #[error("Not a number: the {name:?} value must be a number, not {actual_type}.")]
    InvalidThinEdgeJsonValue { name: String, actual_type: String },

    #[error("Not a timestamp: the time value must be a timestamp string, not {actual_type}.")]
    InvalidThinEdgeJsonTime { actual_type: String },

    #[error(
        "Number out-of-range: the {name:?} value is too large to be represented as a float64."
    )]
    InvalidThinEdgeJsonNumber { name: String },

    #[error("Invalid measurement name: {name:?} is a reserved word.")]
    ThinEdgeReservedWordError { name: String },

    #[error(
        "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): {value:?}: {from}"
    )]
    InvalidTimestamp { value: String, from: ParseError },

    #[error("More than 2 nested levels: the record for {name:?} must be flattened.")]
    InvalidThinEdgeHierarchy { name: String },
}

impl ThinEdgeJsonError {
    const MAX_LEN: usize = 80;

    fn new_invalid_utf8(bytes: &[u8], from: std::str::Utf8Error) -> ThinEdgeJsonError {
        let index = from.valid_up_to();
        let input = std::str::from_utf8(&bytes[..index]).unwrap_or("");

        ThinEdgeJsonError::InvalidUTF8 {
            input_excerpt: input_prefix(input, ThinEdgeJsonError::MAX_LEN),
            from,
        }
    }

    fn new_invalid_json(input: &str, from: json::JsonError) -> ThinEdgeJsonError {
        ThinEdgeJsonError::InvalidJson {
            input_excerpt: input_prefix(input, ThinEdgeJsonError::MAX_LEN),
            from,
        }
    }

    fn new_invalid_json_root(json: &JsonValue) -> ThinEdgeJsonError {
        ThinEdgeJsonError::InvalidThinEdgeJsonRoot {
            json_excerpt: input_prefix(&json.to_string(), ThinEdgeJsonError::MAX_LEN),
            actual_type: ThinEdgeJsonError::json_type(&json).to_string(),
        }
    }

    fn new_invalid_json_value(name: &str, json: &JsonValue) -> ThinEdgeJsonError {
        ThinEdgeJsonError::InvalidThinEdgeJsonValue {
            name: String::from(name),
            actual_type: ThinEdgeJsonError::json_type(&json).to_string(),
        }
    }

    fn new_invalid_json_time(json: &JsonValue) -> ThinEdgeJsonError {
        ThinEdgeJsonError::InvalidThinEdgeJsonTime {
            actual_type: ThinEdgeJsonError::json_type(&json).to_string(),
        }
    }

    fn json_type(input: &JsonValue) -> &'static str {
        match input {
            JsonValue::String(_) | JsonValue::Short(_) => "a string",
            JsonValue::Number(_) => "a number",
            JsonValue::Object(_) => "an object",
            JsonValue::Array(_) => "an array",
            JsonValue::Boolean(_) => "a boolean",
            JsonValue::Null => "null",
        }
    }
}

fn input_prefix(input: &str, len: usize) -> String {
    let input = input.split_whitespace().collect::<String>();
    if input.len() < len {
        input.to_string()
    } else {
        input[0..len].to_string()
    }
}

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn check_single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let utc_time_now: DateTime<Utc> = Utc::now();

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
            utc_time_now.to_rfc3339(),
            body_of_message
        );

        let output = CumulocityJson::from_thin_edge_json_with_timestamp(
            &String::from(single_value_thin_edge_json).into_bytes(),
            utc_time_now,
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
                  "time" : "2013-06-22T17:03:14.000+02:00",
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let expected_output = r#"{
                     "type": "ThinEdgeMeasurement",
                     "time": "2013-06-22T17:03:14.000+02:00",
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
            utc_time_now,
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
    fn thin_edge_json_reject_invalid_utf8() {
        let input = b"temperature\xc3\x28";

        let expected_error =
            r#"Invalid UTF8: invalid utf-8 sequence of 1 bytes from index 11: temperature..."#;
        let output = CumulocityJson::from_thin_edge_json(input);

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_arrays() {
        let input = r"[50,23]";

        let expected_error = r#"Invalid Thin Edge measurement: it cannot be an array: [50,23]"#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_nested_arrays() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": [50,60,70]
          }"#;

        let expected_error =
            r#"Not a number: the "temperature" value must be a number, not an array."#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_string_value() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 50,
           "pressure": "20"
          }"#;

        let expected_error =
            r#"Not a number: the "pressure" value must be a number, not a string."#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_huge_number() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 10e99999
          }"#;

        let expected_error = r#"Number out-of-range: the "temperature" value is too large to be represented as a float64."#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_round_tiny_number() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 10e-9999999999
          }"#;

        let expected_output = r#"{
             "type": "ThinEdgeMeasurement",
             "time": "2013-06-22T17:03:14.000+02:00",
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

    #[test]
    fn thin_edge_json_reject_boolean_value() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": true,
           "pressure": 220
          }"#;

        let expected_output =
            r#"Not a number: the "temperature" value must be a number, not a boolean."#;
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_reject_deep_hierarchy() {
        let multi_level_heirarchy = r#"{
                "location": {
                      "latitude": 32.54,
                      "longitude": -117.67,
                      "altitude": 98.6,
                      "area": {
                         "breadth": 32.54,
                         "depth": 117.67
                      }
                  },
                "pressure": 98
        }"#;
        let expected_output =
            r#"More than 2 nested levels: the record for "area" must be flattened."#;
        let output =
            CumulocityJson::from_thin_edge_json(&String::from(multi_level_heirarchy).into_bytes());
        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_reject_measurement_named_type() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "type": 40,
           "pressure": 220
          }"#;

        let expected_output = r#"Invalid measurement name: "type" is a reserved word."#;
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_reject_number_for_time() {
        let string_value_thin_edge_json = r#"{
           "time": 40,
           "pressure": 220
          }"#;

        let expected_output =
            r#"Not a timestamp: the time value must be a timestamp string, not a number."#;
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_invalid_json() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "pressure": 220;
          }"#;

        let expected_error = r#"Invalid JSON: Unexpected character: ; at (3:27): {"time":"2013-06-22T17:03:14.000+02:00","pressure":220;}"#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_partial_json() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
        "#;

        let expected_error =
            r#"Invalid JSON: Unexpected end of JSON: {"time":"2013-06-22T17:03:14.000+02:00","#;
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_partial_timestamp() {
        let input = r#"{
           "time" : "2013-06-22",
           "pressure": 220
          }"#;

        let expected_error = "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): \"2013-06-22\": premature end of input";
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_invalid_timestamp() {
        let input = r#"{
           "time" : "2013-06-22 3am",
           "pressure": 220
          }"#;

        let expected_error =
            "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): \"2013-06-22 3am\": input contains invalid characters";
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    use proptest::prelude::*;

    proptest! {
            #[test]
            fn it_works_for_any_measurement(measurement in r#"[a-z]{3,6}"#) {
                let input = format!(r#""time: "2013-06-22T17:03:14.000+02:00",{{
                            "{}": 123
                          }}"#, measurement);
                let time = "2013-06-22T17:03:14.000+02:00";
                let time_utc : DateTime<Utc> = time.parse().unwrap();
                let expected_output = format!(r#"{{
                                  "type": "ThinEdgeMeasurement",
                                  "time": "{}",
                                  "{}": {{
                                  "{}": {{
                                  "value": 123
                                 }}
                                 }}
                                }}"#, time_utc.to_rfc3339(), measurement, measurement);


        match CumulocityJson::from_thin_edge_json(
                    &String::from(input).into_bytes(),
                ) {
                    Ok(vec) => {
                        assert_eq!(
                            expected_output.split_whitespace().collect::<String>(),
                            String::from_utf8(vec)
                                .unwrap()
                                .split_whitespace()
                                .collect::<String>()
                        );
                    }
                    Err(e) => {
                        eprintln!("Error is {}", e);
                    }
                }
        }
    }
}
