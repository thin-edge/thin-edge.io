//! A library to create the ThinEdgeJson from bytes of json data by Validating it.
//! It also serialize the ThinEdgeJson data.

use chrono::{format::ParseError, prelude::*};
use json::JsonValue;

use crate::measurement::GroupedMeasurementVisitor;

/// ThinEdgeJson is represented in this struct
/// Since json does not understand DateTime format, the time stamp is represented as a string
/// Before populating the struct members the thin edge json values and names will be validated
#[derive(Debug)]
pub struct ThinEdgeJson {
    pub timestamp: Option<DateTime<FixedOffset>>,
    pub values: Vec<ThinEdgeValue>,
}
#[derive(Debug)]
pub enum ThinEdgeValue {
    Single(SingleValueMeasurement),
    Multi(MultiValueMeasurement),
}
#[derive(Debug)]
pub struct SingleValueMeasurement {
    pub name: String,
    pub value: f64,
}

impl SingleValueMeasurement {
    fn new(name: impl Into<String>, value: f64) -> Result<Self, ThinEdgeJsonError> {
        if value == 0.0 || value.is_normal() {
            let single_value = SingleValueMeasurement {
                name: name.into(),
                value,
            };
            Ok(single_value)
        } else {
            Err(ThinEdgeJsonError::InvalidThinEdgeJsonNumber { name: name.into() })
        }
    }
}

#[derive(Debug)]
pub struct MultiValueMeasurement {
    pub name: String,
    pub values: Vec<SingleValueMeasurement>,
}

impl MultiValueMeasurement {
    fn new(
        name: impl Into<String>,
        multi_value_obj: &json::object::Object,
    ) -> Result<Self, ThinEdgeJsonError> {
        let mut single_values = vec![];

        for (k, v) in multi_value_obj.iter() {
            match v {
                JsonValue::Number(num) => {
                    // Single Value object
                    let single_value_measurement = SingleValueMeasurement::new(k, (*num).into())?;
                    single_values.push(single_value_measurement);
                }
                JsonValue::Object(_object) => {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeHierarchy { name: k.into() })
                }
                value => {
                    return Err(ThinEdgeJsonError::new_invalid_json_value(
                        &name.into(),
                        value,
                    ));
                }
            }
        }
        if single_values.is_empty() {
            Err(ThinEdgeJsonError::EmptyThinEdgeJson { name: name.into() })
        } else {
            Ok(MultiValueMeasurement {
                name: name.into(),
                values: single_values,
            })
        }
    }
}

struct ThinEdgeJsonBuilder {
    timestamp: Option<DateTime<FixedOffset>>,
    inside_group: Option<MultiValueMeasurement>,
    measurements: Vec<ThinEdgeValue>,
}

impl ThinEdgeJsonBuilder {
    fn new() -> Self {
        Self {
            timestamp: None,
            inside_group: None,
            measurements: Vec::new(),
        }
    }

    fn finish(self) -> Result<ThinEdgeJson, ThinEdgeJsonBuilderError> {
        assert!(self.inside_group.is_none());

        Ok(ThinEdgeJson {
            timestamp: self.timestamp,
            values: self.measurements,
        })
    }
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum ThinEdgeJsonBuilderError {
    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,

    #[error("... time stamp within a group")]
    DuplicatedTimestamp,

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,
}

impl GroupedMeasurementVisitor for ThinEdgeJsonBuilder {
    type Error = ThinEdgeJsonBuilderError;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        if self.timestamp.is_some() {
            return Err(ThinEdgeJsonBuilderError::DuplicatedTimestamp);
        }
        self.timestamp = Some(value);
        Ok(())
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        if let Some(group) = &mut self.inside_group {
            group.values.push(SingleValueMeasurement {
                name: name.into(),
                value,
            });
        } else {
            self.measurements
                .push(ThinEdgeValue::Single(SingleValueMeasurement {
                    name: name.into(),
                    value,
                }));
        }
        Ok(())
    }

    fn start_group(&mut self, group: &str) -> Result<(), Self::Error> {
        if self.inside_group.is_none() {
            self.inside_group = Some(MultiValueMeasurement {
                name: group.into(),
                values: Vec::new(),
            });
            Ok(())
        } else {
            Err(ThinEdgeJsonBuilderError::/*NestedGroupsNotSupported*/UnexpectedStartOfGroup)
        }
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        match self.inside_group.take() {
            Some(group) => self.measurements.push(ThinEdgeValue::Multi(group)),
            None => return Err(ThinEdgeJsonBuilderError::UnexpectedEndOfGroup),
        }
        Ok(())
    }
}

struct ThinEdgeJsonParser;

impl ThinEdgeJsonParser {
    /// Confirms that the json is in thin-edge json format or not
    fn accept_json<T: GroupedMeasurementVisitor>(
        input: json::JsonValue,
        visitor: &mut T,
    ) -> Result<(), ThinEdgeJsonError>
    where
        ThinEdgeJsonError: From<T::Error>,
    {
        match &input {
            JsonValue::Object(thin_edge_obj) => {
                for (key, value) in thin_edge_obj.iter() {
                    if key.eq("type") {
                        return Err(ThinEdgeJsonError::ThinEdgeReservedWordError {
                            name: String::from(key),
                        });
                    } else if key.eq("time") {
                        let () = visitor.timestamp(parse_timestamp_iso8601(value)?)?;
                    } else {
                        match value {
                            // Single Value object
                            JsonValue::Number(num) => {
                                let () = visitor.measurement(key, (*num).into())?;
                            }
                            // Multi value object
                            JsonValue::Object(multi_value_thin_edge_object) => {
                                let () = visitor.start_group(key)?;

                                for (k, v) in multi_value_thin_edge_object.iter() {
                                    match v {
                                        JsonValue::Number(num) => {
                                            // Single Value object
                                            let () = visitor.measurement(k, (*num).into())?;
                                        }
                                        JsonValue::Object(_object) => {
                                            return Err(
                                                ThinEdgeJsonError::InvalidThinEdgeHierarchy {
                                                    name: k.into(),
                                                },
                                            )
                                        }
                                        value => {
                                            return Err(ThinEdgeJsonError::new_invalid_json_value(
                                                k.into(),
                                                value,
                                            ));
                                        }
                                    }
                                }

                                let () = visitor.end_group()?;
                            }

                            _ => {
                                return Err(ThinEdgeJsonError::new_invalid_json_value(key, value));
                            }
                        }
                    }
                }
            }
            _ => return Err(ThinEdgeJsonError::new_invalid_json_root(&input)),
        }

        Ok(())
    }
}

impl ThinEdgeJson {
    pub fn from_utf8(
        input: &[u8],
        timestamp: DateTime<FixedOffset>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let json_string = std::str::from_utf8(input)
            .map_err(|err| ThinEdgeJsonError::new_invalid_utf8(input, err))?;
        ThinEdgeJson::from_str(json_string, timestamp)
    }

    pub fn from_str(
        json_string: &str,
        timestamp: DateTime<FixedOffset>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        match json::parse(&json_string) {
            Ok(thin_edge_obj) => ThinEdgeJson::from_json(thin_edge_obj, timestamp),
            Err(err) => Err(ThinEdgeJsonError::new_invalid_json(json_string, err)),
        }
    }

    /// Confirms that the json is in thin-edge json format or not
    fn from_json(
        input: json::JsonValue,
        timestamp: DateTime<FixedOffset>,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        let mut builder = ThinEdgeJsonBuilder::new();
        let () = ThinEdgeJsonParser::accept_json(input, &mut builder)?;
        Ok(builder.finish()?)
    }
}

fn parse_timestamp_iso8601(value: &JsonValue) -> Result<DateTime<FixedOffset>, ThinEdgeJsonError> {
    match value {
        JsonValue::Short(str) => {
            let timestamp = str.as_str();
            //Parse fails if timestamp is not is8601 complaint
            let result = DateTime::parse_from_rfc3339(&timestamp).map_err(|err| {
                ThinEdgeJsonError::InvalidTimestamp {
                    value: String::from(timestamp),
                    from: err,
                }
            })?;
            Ok(result)
        }
        _ => Err(ThinEdgeJsonError::new_invalid_json_time(value)),
    }
}

fn input_prefix(input: &str, len: usize) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(len)
        .collect()
}

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum ThinEdgeJsonError {
    #[error("Invalid UTF8: {from}: {input_excerpt}...")]
    InvalidUtf8 {
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

    #[error("Empty Thin Edge measurement: it must contain at least one measurement")]
    EmptyThinEdgeJsonRoot,

    #[error("Empty Thin Edge measurement: {name:?} must contain at least one measurement")]
    EmptyThinEdgeJson { name: String },

    #[error("Not a number: the {name:?} value must be a number, not {actual_type}.")]
    InvalidThinEdgeJsonValue { name: String, actual_type: String },

    #[error("Not a timestamp: the time value must be an ISO8601 timestamp string in the YYYY-MM-DDThh:mm:ss.sss.±hh:mm format, not {actual_type}.")]
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

    #[error(transparent)]
    ThinEdgeJsonBuilderError(#[from] ThinEdgeJsonBuilderError),
}

impl ThinEdgeJsonError {
    const MAX_LEN: usize = 80;

    fn new_invalid_utf8(bytes: &[u8], from: std::str::Utf8Error) -> ThinEdgeJsonError {
        let index = from.valid_up_to();
        let input = std::str::from_utf8(&bytes[..index]).unwrap_or("");

        ThinEdgeJsonError::InvalidUtf8 {
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

#[cfg(test)]

mod tests {
    use super::*;

    fn test_timestamp() -> DateTime<FixedOffset> {
        FixedOffset::east(5 * 3600)
            .ymd(2021, 04, 08)
            .and_hms(0, 0, 0)
    }

    #[test]
    fn thin_edge_json_reject_invalid_utf8() {
        let input = b"temperature\xc3\x28";

        let expected_error =
            r#"Invalid UTF8: invalid utf-8 sequence of 1 bytes from index 11: temperature..."#;
        let output = ThinEdgeJson::from_utf8(input, test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_non_utf8_input() {
        let input = b"\xc3\x28";

        let expected_error = r#"Invalid UTF8: invalid utf-8 sequence of 1 bytes from index 0: ..."#;
        let output = ThinEdgeJson::from_utf8(input, test_timestamp());
        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_arrays() {
        let input = r"[50,23]";

        let expected_error = r#"Invalid Thin Edge measurement: it cannot be an array: [50,23]"#;
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
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
        let output =
            ThinEdgeJson::from_utf8(string_value_thin_edge_json.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(multi_level_heirarchy.as_bytes(), test_timestamp());
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
        let output =
            ThinEdgeJson::from_utf8(string_value_thin_edge_json.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_reject_number_for_time() {
        let string_value_thin_edge_json = r#"{
           "time": 40,
           "pressure": 220
          }"#;

        let expected_output = r#"Not a timestamp: the time value must be an ISO8601 timestamp string in the YYYY-MM-DDThh:mm:ss.sss.±hh:mm format, not a number."#;
        let output =
            ThinEdgeJson::from_utf8(string_value_thin_edge_json.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_empty_record() {
        let input = "{}";

        let expected_error =
            "Empty Thin Edge measurement: it must contain at least one measurement";
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_just_time() {
        let input = r#"{
           "time" : "2013-06-22T17:03:14.000+02:00"
        }"#;

        let expected_error =
            "Empty Thin Edge measurement: it must contain at least one measurement";
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_empty_measurement() {
        let input = r#"{
           "foo" : {}
        }"#;

        let expected_error =
            r#"Empty Thin Edge measurement: "foo" must contain at least one measurement"#;
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

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
        let output = ThinEdgeJson::from_utf8(input.as_bytes(), test_timestamp());

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn prefix_fn_removes_extra_chars() {
        let input = "薄いエッジ";
        assert_eq!(input.len(), 15);
        assert_eq!(input_prefix(input, 1), "薄");
    }

    #[test]
    fn prefix_fn_let_unchanged_short_inputs() {
        let input = "FØØ";
        assert_eq!(input.len(), 5);
        assert_eq!(input_prefix(input, 4), input);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prefix_doesnt_crash(input in "\\PC*") {
            let _ = input_prefix(&input, 10);
        }
    }
}
