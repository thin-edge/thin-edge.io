//! A library to translate the ThinEdgeJson into C8yJson
//!
//! use c8y_json_translator::ThinEdgeJson;
//!fn main() {
//!let single_value_thinedge_json = r#"{
//!                  "temperature": 23,
//!                  "pressure": 220
//!               }"#;
//!        let time = "2020-06-22T17:03:14.000+02:00";
//!        let msg_type = "SingleValueThinEdgeMeasurement";
//!        let c8y_json = ThinEdgeJson::from_utf8(&String::from(single_value_thinedge_json)
//!                                                         .into_bytes())
//!                                                         .unwrap()
//!                                                         .into_cumulocity_json(time, msg_type);

use json::JsonValue;
use std::error;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum JsonError {
    InvalidUTF8(std::str::Utf8Error),
    InvalidJson(json::Error),
    InvalidThinEdgeJson { name: String },
    InvalidThinEdgeJsonValue { name: String },
}

pub struct ThinEdgeJson {
    values: Vec<ThinEdgeValue>,
}

enum ThinEdgeValue {
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}

pub struct SingleValueMeasurement {
    name: String,
    value: json::number::Number,
}

struct MultiValueMeasurement {
    name: String,
    values: Vec<SingleValueMeasurement>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct CumulocityJson {
    c8yjson: JsonValue,
}

impl ThinEdgeJson {
    ///From array of bytes->to str->convert then to json
    pub fn from_utf8(input: &[u8]) -> Result<ThinEdgeJson, JsonError> {
        let json_string = std::str::from_utf8(input)?;
        match json::parse(&json_string) {
            //Check the object for the thin -edge json template 2 level
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj),
            Err(err) => {
                eprintln!("Error while creating the JsonValue");
                Err(JsonError::InvalidJson(err))
            }
        }
    }

    ///Confirms that the json is in thin-edge json format or not
    pub fn from_json(input: json::JsonValue) -> Result<ThinEdgeJson, JsonError> {
        let mut measurements: Vec<ThinEdgeValue> = Vec::new();
        match input.clone() {
            json::JsonValue::Object(thin_edge_obj) => {
                for (k, v) in thin_edge_obj.iter() {
                    match v {
                        JsonValue::Number(num) => {
                            //Single Value object
                            match create_single_val_thinedge_struct(k, *num) {
                                Ok(single_value_measurement) => measurements
                                    .push(ThinEdgeValue::Single(single_value_measurement)),
                                Err(e) => return Err(e),
                            }
                        }
                        JsonValue::Object(multi_value_thin_obj) => {
                            //Multi value object
                            match create_multi_val_thinedge_struct(multi_value_thin_obj, k) {
                                Ok(multi_value_measurement) => {
                                    measurements.push(ThinEdgeValue::Multi(multi_value_measurement))
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        _ => {
                            eprintln!(" Error: Invalid thin edge json ");
                            return Err(JsonError::InvalidThinEdgeJson {
                                name: String::from(k),
                            });
                        }
                    }
                }
            }
            _ => {
                eprintln!("Error: Not a multi-value object");
                return Err(JsonError::InvalidThinEdgeJson {
                    name: input.to_string(),
                });
            }
        };
        Ok(ThinEdgeJson {
            values: measurements,
        })
    }

    ///Convert from thinedgejson to c8yjson
    pub fn into_cumulocity_json(self, timestamp: &str, c8ytype: &str) -> CumulocityJson {
        let mut c8yobj = create_c8yjson_object(timestamp, c8ytype);

        for v in self.values.iter() {
            match v {
                ThinEdgeValue::Single(single_value_measurement) => {
                    c8yobj
                        .c8yjson
                        .insert(
                            &single_value_measurement.name,
                            translate_single_value_object(single_value_measurement),
                        )
                        .unwrap();
                }
                ThinEdgeValue::Multi(multi_value_measurement) => {
                    c8yobj
                        .c8yjson
                        .insert(
                            &multi_value_measurement.name,
                            translate_multivalue_object(multi_value_measurement),
                        )
                        .unwrap();
                }
            }
        }
        c8yobj
    }
}

fn create_single_val_thinedge_struct(
    name: &str,
    value: json::number::Number,
) -> Result<SingleValueMeasurement, JsonError> {
    let num: f64 = (value).into();
    if num == 0.0 || num.is_normal() {
        let single_value = SingleValueMeasurement {
            name: String::from(name),
            value,
        };
        Ok(single_value)
    } else {
        Err(JsonError::InvalidThinEdgeJsonValue {
            name: String::from(name),
        })
    }
}

fn create_multi_val_thinedge_struct(
    multi_value_obj: &json::object::Object,
    name: &str,
) -> Result<MultiValueMeasurement, JsonError> {
    let mut single_value: Vec<SingleValueMeasurement> = Vec::new();

    for (k, v) in multi_value_obj.iter() {
        match v {
            JsonValue::Number(num) => {
                //Single Value object
                match create_single_val_thinedge_struct(k, *num) {
                    Ok(single_value_measurement) => single_value.push(single_value_measurement),
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
    Ok(MultiValueMeasurement {
        name: String::from(name),
        values: single_value,
    })
}

fn translate_multivalue_object(multi: &MultiValueMeasurement) -> JsonValue {
    let mut complex_obj: JsonValue = JsonValue::new_object();
    for s in multi.values.iter() {
        create_value_obj_and_insert_into_jsonobj(&s.name, s.value, &mut complex_obj);
        complex_obj
            .insert(&s.name, create_value_obj(json::from(s.value)))
            .unwrap();
    }
    complex_obj
}

fn translate_single_value_object(single: &SingleValueMeasurement) -> JsonValue {
    let mut single_value_object: JsonValue = JsonValue::new_object();
    single_value_object
        .insert(&single.name, create_value_obj(json::from(single.value)))
        .unwrap();
    single_value_object
}

fn create_value_obj_and_insert_into_jsonobj(
    key: &str,
    num: json::number::Number,
    jsonobj: &mut JsonValue,
) {
    match jsonobj.insert(key, create_value_obj(json::from(num))) {
        Ok(_obj) => _obj,
        Err(_e) => eprintln!("Failed to insert the json object"),
    }
}

fn create_value_obj(value: JsonValue) -> JsonValue {
    let mut valueobj = JsonValue::new_object();
    valueobj.insert("value", value).unwrap();
    valueobj
}

fn create_c8yjson_object(timestamp: &str, c8y_msg_type: &str) -> CumulocityJson {
    let mut c8yobj: JsonValue = JsonValue::new_object();
    c8yobj.insert("type", c8y_msg_type).unwrap();
    c8yobj.insert("time", timestamp).unwrap();
    CumulocityJson { c8yjson: c8yobj }
}

impl fmt::Display for CumulocityJson {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#}", self.c8yjson)
    }
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
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    #[test]
    fn single_value_translation() {
        let single_value_thinedge_json = r#"{
                  "temperature": 23,
                  "pressure": 220
               }"#;

        let time = "2020-06-22T17:03:14.000+02:00";
        let msg_type = "SingleValueThinEdgeMeasurement";

        //        println!("Tedge_Json: {:#}", single_value_thinedge_json);

        let expected_output = r#"{
         "type": "SingleValueThinEdgeMeasurement",
         "time": "2020-06-22T17:03:14.000+02:00",
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
         }"#;

        let out_put =
            ThinEdgeJson::from_utf8(&String::from(single_value_thinedge_json).into_bytes())
                .unwrap()
                .into_cumulocity_json(time, msg_type)
                .to_string();

        let expected = expected_output.split_whitespace().collect::<String>();
        let output = out_put.split_whitespace().collect::<String>();

        println!("{}", expected);
        println!("{}", output);

        assert_eq!(expected, output);
    }

    #[test]
    fn multi_value_translation() {
        let time = "2020-06-22T17:03:14.000+02:00";
        let msg_type = "MultiValueThinEdgeMeasurement";

        let input = r#"{
                "temperature": 25 ,
                "location": {
                      "latitude": 32.54,
                      "longitude": -117.67,
                      "altitude": 98.6
                  },
                "pressure": 98
    }"#;

        let expected_output = r#"{ 
            "type": "MultiValueThinEdgeMeasurement",
            "time": "2020-06-22T17:03:14.000+02:00",

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
     }"#;

        let output = ThinEdgeJson::from_utf8(&String::from(input).into_bytes())
            .unwrap()
            .into_cumulocity_json(time, msg_type)
            .to_string();

        let expected_string = expected_output.split_whitespace().collect::<String>();
        let output_string = output.split_whitespace().collect::<String>();

        assert_eq!(expected_string, output_string);
    }
}
