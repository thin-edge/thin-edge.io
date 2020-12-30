//! A library to translate the ThinEdgeJson into C8yJson
//!
//! use c8y_json_translator::ThinEdgeJson;
//!fn main() {
//!let single_value_thin_edge_json = r#"{
//!                  "temperature": 23,
//!                  "pressure": 220
//!               }"#;
//!        let time = "2020-06-22T17:03:14.000+02:00";
//!        let msg_type = "SingleValueThinEdgeMeasurement";
//!        let c8y_json = ThinEdgeJson::from_utf8(&String::from(single_value_thin_edge_json)
//!                                                         .into_bytes())
//!                                                         .unwrap()
//!                                                         .into_cumulocity_json(time, msg_type);

use chrono::prelude::*;
use json::JsonValue;
use std::error;
use std::fmt;

pub struct ThinEdgeJson {
    values: Vec<ThinEdgeValue>,
}

enum ThinEdgeValue {
    TimeStamp(TimeStamp),
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}

pub struct TimeStamp {
    name: String,
    value: String,
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
    ///From array of bytes->to str->convert then to json
    pub fn from_utf8(input: &[u8]) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let json_string = std::str::from_utf8(input)?;
        match json::parse(&json_string) {
            //Check the object for the thin -edge json template 2 level
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj),
            Err(err) => Err(ThinEdgeJsonError::InvalidJson(err)),
        }
    }

    ///Confirms that the json is in thin-edge json format or not
    pub fn from_json(input: json::JsonValue) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let mut measurements = vec![];
        match &input {
            JsonValue::Object(thin_edge_obj) => {
                for (k, v) in thin_edge_obj.iter() {
                    match v {
                        //Single Value object
                        JsonValue::Number(num) => {
                            let single_value_measurement =
                                ThinEdgeValue::create_single_val_thin_edge_struct(
                                    k,
                                    (*num).into(),
                                )?;
                            measurements.push(single_value_measurement)
                        }
                        //Multi value object
                        JsonValue::Object(multi_value_thin_edge_object) => {
                            let multi_value_measurement =
                                ThinEdgeValue::create_multi_val_thin_edge_struct(
                                    k,
                                    multi_value_thin_edge_object,
                                )?;
                            measurements.push(multi_value_measurement)
                        }
                        //Short String value object
                        JsonValue::Short(short_value) => {
                            let short_value_measurement =
                                ThinEdgeValue::create_string_value_thin_edge_struct(
                                    k,
                                    short_value,
                                )?;
                            measurements.push(short_value_measurement);
                        }
                        _ => {
                            return Err(ThinEdgeJsonError::InvalidThinEdgeJson {
                                name: String::from(k),
                            });
                        }
                    }
                }
                Ok(ThinEdgeJson {
                    values: measurements,
                })
            }
            _ => Err(ThinEdgeJsonError::InvalidThinEdgeJson {
                name: input.to_string(),
            }),
        }
    }

    ///Convert from thinedgejson to c8y_json
    pub fn into_cumulocity_json(
        self,
        timestamp: DateTime<Utc>,
        c8y_msg_type: &str,
    ) -> CumulocityJson {
        let mut c8y_object = CumulocityJson::new(timestamp, c8y_msg_type);

        for v in self.values.iter() {
            match v {
                ThinEdgeValue::Single(thin_edge_single_value_measurement) => {
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &thin_edge_single_value_measurement.name,
                        CumulocityJson::translate_into_c8y_single_value_object(
                            thin_edge_single_value_measurement,
                        ),
                    );
                }
                ThinEdgeValue::Multi(thin_edge_multi_value_measurement) => {
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &thin_edge_multi_value_measurement.name,
                        CumulocityJson::translate_into_c8y_multi_value_object(
                            thin_edge_multi_value_measurement,
                        ),
                    );
                }
                ThinEdgeValue::TimeStamp(given_time_stamp) => {
                    c8y_object.c8y_json.remove(&given_time_stamp.name);
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &given_time_stamp.name,
                        given_time_stamp.value.clone().into(),
                    );
                }
            }
        }
        c8y_object
    }
}

impl ThinEdgeValue {
    fn create_single_val_thin_edge_struct(
        name: &str,
        value: f64,
    ) -> Result<ThinEdgeValue, ThinEdgeJsonError> {
        if value == 0.0 || value.is_normal() {
            let single_value = SingleValueMeasurement {
                name: String::from(name),
                value,
            };
            Ok(ThinEdgeValue::Single(single_value))
        } else {
            Err(ThinEdgeJsonError::InvalidThinEdgeJsonValue {
                name: String::from(name),
            })
        }
    }

    fn create_string_value_thin_edge_struct(
        name: &str,
        value: &str,
    ) -> Result<ThinEdgeValue, ThinEdgeJsonError> {
        if value.ne("time") && value.ne("type") && !value.is_empty() {
            if name.eq("time") || name.eq("type") {
                let time_stamp = TimeStamp {
                    name: String::from(name),
                    value: String::from(value),
                };
                Ok(ThinEdgeValue::TimeStamp(time_stamp))
            } else {
                Err(ThinEdgeJsonError::InvalidThinEdgeJsonValue {
                    name: String::from(name),
                })
            }
        } else {
            Err(ThinEdgeJsonError::ThinEdgeReservedWordError {
                value: String::from(value),
            })
        }
    }

    fn create_multi_val_thin_edge_struct(
        name: &str,
        multi_value_obj: &json::object::Object,
    ) -> Result<ThinEdgeValue, ThinEdgeJsonError> {
        let mut single_values = vec![];

        for (k, v) in multi_value_obj.iter() {
            match v {
                JsonValue::Number(num) => {
                    //Single Value object
                    let single_value_measurement =
                        ThinEdgeValue::create_single_val_thin_edge_struct(k, (*num).into())?;
                    if let ThinEdgeValue::Single(single_value_measurement) =
                        single_value_measurement
                    {
                        single_values.push(single_value_measurement);
                    }
                }
                _ => {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeJsonValue {
                        name: String::from(name),
                    })
                }
            }
        }
        Ok(ThinEdgeValue::Multi(MultiValueMeasurement {
            name: String::from(name),
            values: single_values,
        }))
    }
}

impl CumulocityJson {
    pub fn new(timestamp: DateTime<Utc>, c8y_msg_type: &str) -> CumulocityJson {
        let mut c8y_object: JsonValue = JsonValue::new_object();
        insert_into_json_object(&mut c8y_object, "type", c8y_msg_type.into());
        insert_into_json_object(&mut c8y_object, "time", timestamp.to_rfc3339().into());
        CumulocityJson {
            c8y_json: c8y_object,
        }
    }

    pub fn translate_into_c8y_single_value_object(single: &SingleValueMeasurement) -> JsonValue {
        let mut single_value_c8y_object: JsonValue = JsonValue::new_object();
        insert_into_json_object(
            &mut single_value_c8y_object,
            &single.name,
            create_value_object(single.value.into()),
        );
        single_value_c8y_object
    }

    pub fn translate_into_c8y_multi_value_object(multi: &MultiValueMeasurement) -> JsonValue {
        let mut multi_value_c8y_object: JsonValue = JsonValue::new_object();
        for s in multi.values.iter() {
            insert_into_json_object(&mut multi_value_c8y_object, &s.name, s.value.into());
            insert_into_json_object(
                &mut multi_value_c8y_object,
                &s.name,
                create_value_object(s.value.into()),
            );
        }
        multi_value_c8y_object
    }
}

///We can not avoid the unwrap() call here, its sure that the insert call will not fail
///and panic
fn insert_into_json_object(json_object: &mut JsonValue, name: &str, value: JsonValue) {
    json_object.insert(name, value).unwrap();
}

fn create_value_object(value: JsonValue) -> JsonValue {
    let mut value_object = JsonValue::new_object();
    insert_into_json_object(&mut value_object, "value", value);
    value_object
}

impl fmt::Display for CumulocityJson {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#}", self.c8y_json)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ThinEdgeJsonError {
    InvalidUTF8(std::str::Utf8Error),
    InvalidJson(json::Error),
    InvalidThinEdgeJson { name: String },
    InvalidThinEdgeJsonValue { name: String },
    ThinEdgeReservedWordError { value: String },
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
                eprintln!("{} is a reserved word", value);
                None
            }
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

impl fmt::Display for ThinEdgeJsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ThinEdgeJsonError::InvalidUTF8(..) => write!(f, "InvalidUTF8 Error"),
            ThinEdgeJsonError::InvalidJson(..) => write!(f, "InvalidJson Error"),
            ThinEdgeJsonError::InvalidThinEdgeJson { ref name } => {
                write!(f, "InvalidThinEdgeJson {}", name)
            }
            ThinEdgeJsonError::InvalidThinEdgeJsonValue { ref name } => {
                write!(f, "InvalidThinEdgeJsonValue {}", name)
            }
            ThinEdgeJsonError::ThinEdgeReservedWordError { ref value } => {
                write!(f, "{} is a reserved word", value)
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
    fn single_value_translation() {
        let single_value_thin_edge_json = r#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let utc_time_now: DateTime<Utc> = Utc::now();
        let msg_type = "SingleValueThinEdgeMeasurement";

        let type_string = "{\"type\": \"SingleValueThinEdgeMeasurement\",";

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
        let output =
            ThinEdgeJson::from_utf8(&String::from(single_value_thin_edge_json).into_bytes())
                .unwrap()
                .into_cumulocity_json(utc_time_now, msg_type)
                .to_string();

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output.split_whitespace().collect::<String>()
        );
    }

    #[test]
    fn thin_edge_translation_with_time_stamp() {
        let single_value_thin_edge_json = r#"{
                  "type": "ThinEdgeWithTimeStamp",
                  "time" : "2013-06-22T17:03:14.000+02:00",
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let utc_time_now: DateTime<Utc> = Utc::now();
        let msg_type = "SingleValueThinEdgeMeasurementWithTimeStamp";

        let expected_output = r#"{
                     "type": "ThinEdgeWithTimeStamp",
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

        let output =
            ThinEdgeJson::from_utf8(&String::from(single_value_thin_edge_json).into_bytes())
                .unwrap()
                .into_cumulocity_json(utc_time_now, msg_type)
                .to_string();

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output.split_whitespace().collect::<String>()
        );
    }

    #[test]
    fn multi_value_translation() {
        let local_time_now: DateTime<Utc> = Utc::now();
        let msg_type = "MultiValueThinEdgeMeasurement";

        let type_string = "{\"type\": \"MultiValueThinEdgeMeasurement\",";

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
        let output = ThinEdgeJson::from_utf8(&String::from(input).into_bytes())
            .unwrap()
            .into_cumulocity_json(local_time_now, msg_type)
            .to_string();

        assert_eq!(
            expected_output.split_whitespace().collect::<String>(),
            output.split_whitespace().collect::<String>()
        );
    }
}
