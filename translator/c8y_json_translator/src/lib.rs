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
    value: json::number::Number,
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
    pub fn from_utf8(input: &[u8]) -> Result<ThinEdgeJson, JsonError> {
        let json_string = std::str::from_utf8(input)?;
        match json::parse(&json_string) {
            //Check the object for the thin -edge json template 2 level
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj),
            Err(err) => Err(JsonError::InvalidJson(err)),
        }
    }

    ///Confirms that the json is in thin-edge json format or not
    pub fn from_json(input: json::JsonValue) -> Result<ThinEdgeJson, JsonError> {
        let mut measurements = vec![];
        match &input {
            JsonValue::Object(thin_edge_obj) => {
                for (k, v) in thin_edge_obj.iter() {
                    match v {
                        //Single Value object
                        JsonValue::Number(num) => {
                            let single_value_measurement =
                                create_single_val_thinedge_struct(k, *num)?;
                            measurements.push(single_value_measurement)
                        }
                        //Multi value object
                        JsonValue::Object(multi_value_thin_edge_object) => {
                            let multi_value_measurement =
                                create_multi_val_thin_edge_struct(k, multi_value_thin_edge_object)?;
                            measurements.push(multi_value_measurement)
                        }
                        //String value object
                        JsonValue::Short(short_value) => {
                            let short_value_measurement =
                                create_type_and_time_stamp_thin_edge_struct(k, short_value)?;
                            measurements.push(short_value_measurement);
                        }
                        _ => {
                            return Err(JsonError::InvalidThinEdgeJson {
                                name: String::from(k),
                            });
                        }
                    }
                }
                Ok(ThinEdgeJson {
                    values: measurements,
                })
            }
            _ => Err(JsonError::InvalidThinEdgeJson {
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
        let mut c8y_object = create_c8y_json_object(timestamp, c8y_msg_type);

        for v in self.values.iter() {
            match v {
                ThinEdgeValue::Single(single_value_measurement) => {
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &single_value_measurement.name,
                        translate_single_value_object(single_value_measurement),
                    );
                }
                ThinEdgeValue::Multi(multi_value_measurement) => {
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &multi_value_measurement.name,
                        translate_multi_value_object(multi_value_measurement),
                    );
                }
                ThinEdgeValue::TimeStamp(given_time_stamp) => {
                    c8y_object.c8y_json.remove(&given_time_stamp.name);
                    insert_into_json_object(
                        &mut c8y_object.c8y_json,
                        &given_time_stamp.name,
                        given_time_stamp.value.clone().into(),
                    ); //translate_time_stamp(given_time_stamp);
                }
            }
        }
        c8y_object
    }
}

fn create_single_val_thinedge_struct(
    name: &str,
    value: json::number::Number,
) -> Result<ThinEdgeValue, JsonError> {
    let num: f64 = (value).into();
    if num == 0.0 || num.is_normal() {
        let single_value = SingleValueMeasurement {
            name: String::from(name),
            value,
        };
        Ok(ThinEdgeValue::Single(single_value))
    } else {
        Err(JsonError::InvalidThinEdgeJsonValue {
            name: String::from(name),
        })
    }
}

fn create_type_and_time_stamp_thin_edge_struct(
    name: &str,
    value: &str,
) -> Result<ThinEdgeValue, JsonError> {
    if (name == "time" || name == "type") && !value.is_empty() {
        let time_stamp = TimeStamp {
            name: String::from(name),
            value: String::from(value),
        };
        Ok(ThinEdgeValue::TimeStamp(time_stamp))
    } else {
        Err(JsonError::InvalidThinEdgeJsonValue {
            name: String::from(name),
        })
    }
}

fn create_multi_val_thin_edge_struct(
    name: &str,
    multi_value_obj: &json::object::Object,
) -> Result<ThinEdgeValue, JsonError> {
    let mut single_values = vec![];

    for (k, v) in multi_value_obj.iter() {
        match v {
            JsonValue::Number(num) => {
                //Single Value object
                match create_single_val_thinedge_struct(k, *num) {
                    Ok(single_value_measurement) => {
                        if let ThinEdgeValue::Single(single_value_measurement) =
                            single_value_measurement
                        {
                            single_values.push(single_value_measurement)
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            _ => {
                return Err(JsonError::InvalidThinEdgeJsonValue {
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

fn translate_multi_value_object(multi: &MultiValueMeasurement) -> JsonValue {
    let mut multi_value_object: JsonValue = JsonValue::new_object();
    for s in multi.values.iter() {
        create_value_object_and_insert_into_jsonobj(&s.name, s.value, &mut multi_value_object);
        insert_into_json_object(
            &mut multi_value_object,
            &s.name,
            create_value_object(s.value.into()),
        );
    }
    multi_value_object
}

///We can not avoid the unwrap() call here, its sure that the insert call will not fail
///and panic
fn insert_into_json_object(json_object: &mut JsonValue, name: &str, value: JsonValue) {
    json_object.insert(name, value).unwrap();
}

fn translate_single_value_object(single: &SingleValueMeasurement) -> JsonValue {
    let mut single_value_object: JsonValue = JsonValue::new_object();
    insert_into_json_object(
        &mut single_value_object,
        &single.name,
        create_value_object(single.value.into()),
    );
    single_value_object
}

/*
fn translate_time_stamp(given_time_stamp: &TimeStamp) -> JsonValue {
    let mut time_stamp_object = JsonValue::new_object();
    insert_into_json_object(&mut time_stamp_object, &given_time_stamp.name, given_time_stamp.value.clone().into());
    time_stamp_object
}
*/

fn create_value_object_and_insert_into_jsonobj(
    key: &str,
    num: json::number::Number,
    json_object: &mut JsonValue,
) {
    insert_into_json_object(json_object, key, num.into());
}

fn create_value_object(value: JsonValue) -> JsonValue {
    let mut value_object = JsonValue::new_object();
    insert_into_json_object(&mut value_object, "value", value);
    value_object
}

fn create_c8y_json_object(timestamp: DateTime<Utc>, c8y_msg_type: &str) -> CumulocityJson {
    let mut c8y_object: JsonValue = JsonValue::new_object();
    insert_into_json_object(&mut c8y_object, "type", c8y_msg_type.into());
    insert_into_json_object(&mut c8y_object, "time", timestamp.to_rfc3339().into());
    CumulocityJson {
        c8y_json: c8y_object,
    }
}

impl fmt::Display for CumulocityJson {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#}", self.c8y_json)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum JsonError {
    InvalidUTF8(std::str::Utf8Error),
    InvalidJson(json::Error),
    InvalidThinEdgeJson { name: String },
    InvalidThinEdgeJsonValue { name: String },
}

impl error::Error for JsonError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            JsonError::InvalidJson(ref e) => Some(e),
            JsonError::InvalidUTF8(ref e) => Some(e),
            JsonError::InvalidThinEdgeJson { ref name } => {
                eprintln!("InvalidThinEdgeJson {}", name);
                None
            }
            JsonError::InvalidThinEdgeJsonValue { ref name } => {
                eprintln!("InvalidThinEdgeJsonValue {}", name);
                None
            }
        }
    }
}

impl From<std::str::Utf8Error> for JsonError {
    fn from(error: std::str::Utf8Error) -> Self {
        JsonError::InvalidUTF8(error)
    }
}

impl From<json::Error> for JsonError {
    fn from(error: json::Error) -> Self {
        JsonError::InvalidJson(error)
    }
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            JsonError::InvalidUTF8(..) => write!(f, "InvalidUTF8 Error"),
            JsonError::InvalidJson(..) => write!(f, "InvalidJson Error"),
            JsonError::InvalidThinEdgeJson { ref name } => {
                write!(f, "InvalidThinEdgeJson {}", name)
            }
            JsonError::InvalidThinEdgeJsonValue { ref name } => {
                write!(f, "InvalidThinEdgeJsonValue {}", name)
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
