//! A library to translate the ThinEdgeJson into C8yJson
//! Takes thin_edge_json bytes and returns c8y json bytes
//!
//! ```
//! use c8y_json_translator::CumulocityJson;
//! let single_value_thin_edge_json = r#"{
//!        "time": "2020-06-22T17:03:14.000+02:00",
//!        "temperature": 23,
//!        "pressure": 220
//!     }"#;
//! let output = CumulocityJson::from_thin_edge_json(
//!             &String::from(single_value_thin_edge_json).into_bytes());
//! ```

//#[macro_use]
//crate proptest;

use proptest::prelude::*;

use chrono::format::ParseError;
use chrono::prelude::*;
use json::JsonValue;
use std::error;
use std::fmt;

/// ThinEdgeJson is represented in this struct
/// Since json does not understand DateTime format, the time stamp is represented as a string
/// Before populating the struct members the thinedge json values and names will be validated

pub struct ThinEdgeJson {
    time_stamp: String,
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
    pub fn from_utf8(input: &[u8]) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let json_string = std::str::from_utf8(input)?;
        match json::parse(&json_string) {
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj),
            Err(err) => Err(ThinEdgeJsonError::InvalidJson(err)),
        }
    }

    ///Confirms that the json is in thin-edge json format or not
    fn from_json(input: json::JsonValue) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let mut measurements = vec![];
        let mut timestamp = Utc::now().to_rfc3339();
        match &input {
            JsonValue::Object(thin_edge_obj) => {
                for (k, v) in thin_edge_obj.iter() {
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
                        //Short String value object
                        JsonValue::Short(short_value) => {
                            if k.eq("time") {
                                timestamp = ThinEdgeJson::check_timestamp_for_iso8601_complaint(
                                    short_value,
                                )?;
                            } else {
                                return Err(ThinEdgeJsonError::InvalidThinEdgeJson {
                                    name: String::from(k),
                                });
                            }
                        }
                        _ => {
                            return Err(ThinEdgeJsonError::InvalidThinEdgeJson {
                                name: String::from(k),
                            });
                        }
                    }
                }
                Ok(ThinEdgeJson {
                    time_stamp: timestamp,
                    values: measurements,
                })
            }
            _ => Err(ThinEdgeJsonError::InvalidThinEdgeJson {
                name: input.to_string(),
            }),
        }
    }

    fn check_timestamp_for_iso8601_complaint(value: &str) -> Result<String, ThinEdgeJsonError> {
        //Parse fails if timestamp is not is8601 complaint
        DateTime::parse_from_rfc3339(&value)?;
        Ok(String::from(value))
    }
}

impl SingleValueMeasurement {
    fn new(name: &str, value: f64) -> Result<Self, ThinEdgeJsonError> {
        if name.ne("time") && name.ne("type") {
            if value == 0.0 || value.is_normal() {
                let single_value = SingleValueMeasurement {
                    name: String::from(name),
                    value,
                };
                Ok(single_value)
            } else {
                Err(ThinEdgeJsonError::InvalidThinEdgeJsonValue {
                    name: String::from(name),
                })
            }
        } else {
            Err(ThinEdgeJsonError::ThinEdgeReservedWordError {
                value: String::from(name),
            })
        }
    }
}

impl MultiValueMeasurement {
    fn new(name: &str, multi_value_obj: &json::object::Object) -> Result<Self, ThinEdgeJsonError> {
        let mut single_values = vec![];

        for (k, v) in multi_value_obj.iter() {
            println!("k: {}", k);
            match v {
                JsonValue::Number(num) => {
                    //Single Value object
                    let single_value_measurement = SingleValueMeasurement::new(k, (*num).into())?;
                    single_values.push(single_value_measurement);
                }
                JsonValue::Object(_object) => {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeHierarchy {
                        name: String::from(name),
                    })
                }
                _ => {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeJsonValue {
                        name: String::from(name),
                    })
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
        let measurements = ThinEdgeJson::from_utf8(input)?;
        let mut c8y_object = CumulocityJson::new(&measurements.time_stamp, "ThinEdgeMeasurement");
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

#[derive(Debug, Eq, PartialEq)]
pub enum ThinEdgeJsonError {
    InvalidUTF8(std::str::Utf8Error),
    InvalidJson(json::Error),
    InvalidThinEdgeJson { name: String },
    InvalidThinEdgeJsonValue { name: String },
    ThinEdgeReservedWordError { value: String },
    InvalidTimeStamp(ParseError),
    InvalidThinEdgeHierarchy { name: String },
}

impl error::Error for ThinEdgeJsonError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            ThinEdgeJsonError::InvalidJson(ref e) => Some(e),
            ThinEdgeJsonError::InvalidUTF8(ref e) => Some(e),
            ThinEdgeJsonError::InvalidThinEdgeJson { ref name } => {
                eprintln!("InvalidThinEdgeJson {}", name);
                None
            }
            ThinEdgeJsonError::InvalidThinEdgeJsonValue { ref name } => {
                eprintln!("InvalidThinEdgeJsonValue {}", name);
                None
            }
            ThinEdgeJsonError::ThinEdgeReservedWordError { ref value } => {
                eprintln!("ThinEdgeReservedWordError: {} is a reserved word", value);
                None
            }
            ThinEdgeJsonError::InvalidThinEdgeHierarchy { ref name } => {
                eprintln!("ThinEdgeHierarchy Error: {} is a reserved word", name);
                None
            }

            ThinEdgeJsonError::InvalidTimeStamp(ref e) => Some(e),
        }
    }
}

impl From<std::str::Utf8Error> for ThinEdgeJsonError {
    fn from(error: std::str::Utf8Error) -> Self {
        ThinEdgeJsonError::InvalidUTF8(error)
    }
}

impl From<json::Error> for ThinEdgeJsonError {
    fn from(error: json::Error) -> Self {
        ThinEdgeJsonError::InvalidJson(error)
    }
}

impl From<chrono::format::ParseError> for ThinEdgeJsonError {
    fn from(error: chrono::format::ParseError) -> Self {
        ThinEdgeJsonError::InvalidTimeStamp(error)
    }
}

impl fmt::Display for ThinEdgeJsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ThinEdgeJsonError::InvalidUTF8(..) => write!(f, "InvalidUTF8 Error"),
            ThinEdgeJsonError::InvalidJson(..) => write!(f, "InvalidJson Error"),
            ThinEdgeJsonError::InvalidTimeStamp(..) => write!(f, "InvalidTimeStamp Error"),
            ThinEdgeJsonError::InvalidThinEdgeJson { ref name } => {
                write!(f, "InvalidThinEdgeJson {}", name)
            }
            ThinEdgeJsonError::InvalidThinEdgeJsonValue { ref name } => {
                write!(f, "InvalidThinEdgeJsonValue {}", name)
            }
            ThinEdgeJsonError::ThinEdgeReservedWordError { ref value } => {
                write!(f, "{} is a reserved word, takes only string value", value)
            }
            ThinEdgeJsonError::InvalidThinEdgeHierarchy { ref name } => {
                write!(f, "InvalidThinEdgeHierarchy, at {}", name)
            }
        }
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

        let output = CumulocityJson::from_thin_edge_json(
            &String::from(single_value_thin_edge_json).into_bytes(),
        );
        match output {
            Ok(vec) => {
                assert_ne!(
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

    #[test]
    fn check_thin_edge_translation_with_time_stamp() {
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
        match output {
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

    #[test]
    fn check_multi_value_translation() {
        let local_time_now: DateTime<Utc> = Utc::now();
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
            local_time_now.to_rfc3339(),
            body_of_message
        );
        let output = CumulocityJson::from_thin_edge_json(&String::from(input).into_bytes());
        match output {
            Ok(vec) => {
                assert_ne!(
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

    #[test]
    //Thin-edge-json should not have string value, except type and time
    fn check_thin_edge_json_with_string_value() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": 50,
           "pressure": "20"
          }"#;

        let expected_output = "InvalidThinEdgeJson pressure";
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }

    #[test]
    //Thin-edge-json should not have boolean value
    fn check_thin_edge_json_with_boolean_value() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "temperature": true,
           "pressure": 220
          }"#;

        let expected_output = "InvalidThinEdgeJson temperature";
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }

    #[test]
    //Thin-edge-json supports one level of heirarchy
    fn check_thin_edge_with_more_than_1_level_heirarchy() {
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
        let expected_output = "InvalidThinEdgeHierarchy, at location";
        let output =
            CumulocityJson::from_thin_edge_json(&String::from(multi_level_heirarchy).into_bytes());

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }
    #[test]
    //Thin-edge-json should not have reserved words type as keys
    fn check_type_reserved_word_as_key() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "type": 40,
           "pressure": 220
          }"#;

        let expected_output = "type is a reserved word, takes only string value";
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }

    #[test]
    //Thin-edge-json should not have reserved words time as keys
    fn check_time_reserved_word_as_key() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "time": 40,
           "pressure": 220
          }"#;

        let expected_output = "time is a reserved word, takes only string value";
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }
    #[test]
    //Invalid json
    fn check_invalid_json_format() {
        let string_value_thin_edge_json = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00",
           "time": 40,
           "pressure": 220;
          }"#;

        let expected_output = "InvalidJson Error";
        let output = CumulocityJson::from_thin_edge_json(
            &String::from(string_value_thin_edge_json).into_bytes(),
        );

        match output {
            Err(e) => {
                assert_eq!(expected_output, e.to_string());
            }
            _ => {}
        }
    }
}
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
