use crate::measurement::GroupedMeasurementVisitor;
use chrono::{format::ParseError, prelude::*};
use json::JsonValue;

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

    fn done(self) -> Result<ThinEdgeJson, ThinEdgeJsonError> {
        if self.inside_group.is_some() {
            return Err(ThinEdgeJsonError::UnexpectedOpenGroup);
        }

        if self.measurements.is_empty() {
            return Err(ThinEdgeJsonError::EmptyThinEdgeJsonRoot);
        }

        Ok(ThinEdgeJson {
            timestamp: self.timestamp,
            values: self.measurements,
        })
    }
}

impl GroupedMeasurementVisitor for ThinEdgeJsonBuilder {
    type Error = ThinEdgeJsonError;

    fn timestamp(&mut self, value: DateTime<FixedOffset>) -> Result<(), Self::Error> {
        match self.timestamp {
            None => {
                self.timestamp = Some(value);
                Ok(())
            }
            Some(_) => Err(ThinEdgeJsonError::DuplicatedTimestamp),
        }
    }

    fn measurement(&mut self, name: &str, value: f64) -> Result<(), Self::Error> {
        let measurement = SingleValueMeasurement::new(name, value)?;
        if let Some(group) = &mut self.inside_group {
            group.values.push(measurement);
        } else {
            self.measurements.push(ThinEdgeValue::Single(measurement));
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
            Err(ThinEdgeJsonError::UnexpectedStartOfGroup)
        }
    }

    fn end_group(&mut self) -> Result<(), Self::Error> {
        match self.inside_group.take() {
            Some(group) => {
                if group.values.is_empty() {
                    return Err(ThinEdgeJsonError::EmptyThinEdgeJson { name: group.name });
                } else {
                    self.measurements.push(ThinEdgeValue::Multi(group))
                }
            }
            None => return Err(ThinEdgeJsonError::UnexpectedEndOfGroup),
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonParserError<T: std::error::Error + std::fmt::Debug + 'static> {
    #[error(transparent)]
    ThinEdgeJsonError(#[from] ThinEdgeJsonError),

    #[error(transparent)]
    VisitorError(T),
}

pub fn parse_str<T: GroupedMeasurementVisitor>(
    json_string: &str,
    visitor: &mut T,
) -> Result<(), ThinEdgeJsonParserError<T::Error>> {
    let thin_edge_obj = json::parse(json_string)
        .map_err(|err| ThinEdgeJsonError::new_invalid_json(json_string, err))?;

    match &thin_edge_obj {
        JsonValue::Object(thin_edge_obj) => {
            for (key, value) in thin_edge_obj.iter() {
                if key.contains('\\') {
                    return Err(ThinEdgeJsonError::InvalidThinEdgeJsonKey {
                        key: String::from(key),
                    }
                    .into());
                }
                if key.eq("type") {
                    return Err(ThinEdgeJsonError::ThinEdgeReservedWordError {
                        name: String::from(key),
                    }
                    .into());
                } else if key.eq("time") {
                    let () = visitor
                        .timestamp(parse_from_rfc3339(
                            value
                                .as_str()
                                .ok_or_else(|| ThinEdgeJsonError::new_invalid_json_time(value))?,
                        )?)
                        .map_err(ThinEdgeJsonParserError::VisitorError)?;
                } else {
                    match value {
                        // Single Value object
                        JsonValue::Number(num) => {
                            let () = visitor
                                .measurement(key, (*num).into())
                                .map_err(ThinEdgeJsonParserError::VisitorError)?;
                        }
                        // Multi value object
                        JsonValue::Object(multi_value_thin_edge_object) => {
                            let () = visitor
                                .start_group(key)
                                .map_err(ThinEdgeJsonParserError::VisitorError)?;

                            for (k, v) in multi_value_thin_edge_object.iter() {
                                match v {
                                    JsonValue::Number(num) => {
                                        // Single Value object
                                        let () = visitor
                                            .measurement(k, (*num).into())
                                            .map_err(ThinEdgeJsonParserError::VisitorError)?;
                                    }
                                    JsonValue::Object(_object) => {
                                        return Err(ThinEdgeJsonError::InvalidThinEdgeHierarchy {
                                            name: k.into(),
                                        }
                                        .into());
                                    }
                                    value => {
                                        return Err(ThinEdgeJsonError::new_invalid_json_value(
                                            k, value,
                                        )
                                        .into());
                                    }
                                }
                            }

                            let () = visitor
                                .end_group()
                                .map_err(ThinEdgeJsonParserError::VisitorError)?;
                        }

                        _ => {
                            return Err(
                                ThinEdgeJsonError::new_invalid_json_value(key, value).into()
                            );
                        }
                    }
                }
            }
        }
        _ => return Err(ThinEdgeJsonError::new_invalid_json_root(&thin_edge_obj).into()),
    }

    Ok(())
}

fn parse_from_rfc3339(timestamp: &str) -> Result<DateTime<FixedOffset>, ThinEdgeJsonError> {
    let time = DateTime::parse_from_rfc3339(&timestamp).map_err(|err| {
        ThinEdgeJsonError::InvalidTimestamp {
            value: String::from(timestamp),
            from: err,
        }
    })?;
    Ok(time)
}

impl ThinEdgeJson {
    pub fn from_str(
        json_string: &str,
    ) -> Result<ThinEdgeJson, ThinEdgeJsonParserError<ThinEdgeJsonError>> {
        let mut builder = ThinEdgeJsonBuilder::new();
        let () = parse_str(json_string, &mut builder)?;
        Ok(builder.done()?)
    }

    pub fn has_timestamp(&self) -> bool {
        self.timestamp.is_some()
    }

    pub fn set_timestamp(&mut self, timestamp: DateTime<FixedOffset>) {
        self.timestamp = Option::from(timestamp)
    }
}

fn input_prefix(input: &str, len: usize) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .take(len)
        .collect()
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonError {
    #[error("... time stamp within a group")]
    DuplicatedTimestamp,

    #[error("Empty Thin Edge measurement: {name:?} must contain at least one measurement")]
    EmptyThinEdgeJson { name: String },

    #[error("Empty Thin Edge measurement: it must contain at least one measurement")]
    EmptyThinEdgeJsonRoot,

    #[error("Invalid JSON: {from}: {input_excerpt}")]
    InvalidJson {
        input_excerpt: String,
        from: json::Error,
    },

    #[error(
        "Number out-of-range: the {name:?} value is too large to be represented as a float64."
    )]
    InvalidThinEdgeJsonNumber { name: String },

    #[error("Invalid Thin Edge measurement: it cannot be {actual_type}: {json_excerpt}")]
    InvalidThinEdgeJsonRoot {
        json_excerpt: String,
        actual_type: String,
    },

    #[error("Invalid Thin Edge key: {key:}")]
    InvalidThinEdgeJsonKey { key: String },

    #[error("Not a timestamp: the time value must be an ISO8601 timestamp string in the YYYY-MM-DDThh:mm:ss.sss.±hh:mm format, not {actual_type}.")]
    InvalidThinEdgeJsonTime { actual_type: String },

    #[error("Not a number: the {name:?} value must be a number, not {actual_type}.")]
    InvalidThinEdgeJsonValue { name: String, actual_type: String },

    #[error("More than 2 nested levels: the record for {name:?} must be flattened.")]
    InvalidThinEdgeHierarchy { name: String },

    #[error(
        "Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): {value:?}: {from}"
    )]
    InvalidTimestamp { value: String, from: ParseError },

    #[error("Invalid UTF8: {from}: {input_excerpt}...")]
    InvalidUtf8 {
        input_excerpt: String,
        from: std::str::Utf8Error,
    },

    #[error("Invalid measurement name: {name:?} is a reserved word.")]
    ThinEdgeReservedWordError { name: String },

    #[error("Unexpected end of group")]
    UnexpectedEndOfGroup,

    #[error("Unexpected open group")]
    UnexpectedOpenGroup,

    #[error("Unexpected start of group")]
    UnexpectedStartOfGroup,

    #[error("Unexpected time stamp within a group")]
    UnexpectedTimestamp,
}

impl ThinEdgeJsonError {
    const MAX_LEN: usize = 80;

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

    #[test]
    fn test_str_with_invalid_timestamp() {
        let input = r#"{
            "time" : "2013-06-2217:03:14.000658767+02:00"
        }"#;
        let expected_error = r#"Invalid ISO8601 timestamp (expected YYYY-MM-DDThh:mm:ss.sss.±hh:mm): "2013-06-2217:03:14.000658767+02:00": input contains invalid characters"#;
        let output_err = ThinEdgeJson::from_str(input).unwrap_err();
        assert_eq!(output_err.to_string(), expected_error);
    }

    #[test]
    fn test_str_with_valid_timestamp() {
        let input = r#"{
            "time" : "2021-04-30T17:03:14+02:00",
            "temperature" : 25
        }"#;
        let output = ThinEdgeJson::from_str(input).unwrap();
        assert_eq!(
            output.timestamp,
            Some(
                FixedOffset::east(2 * 3600)
                    .ymd(2021, 4, 30)
                    .and_hms(17, 3, 14)
            )
        );
    }

    #[test]
    fn test_str_with_millisecond_timestamp() {
        let input = r#"{
            "time" : "2021-04-30T17:03:14.123+02:00",
            "temperature" : 25
        }"#;

        let output = ThinEdgeJson::from_str(input).unwrap();
        assert_eq!(
            output.timestamp,
            Some(
                FixedOffset::east(2 * 3600)
                    .ymd(2021, 4, 30)
                    .and_hms_milli(17, 3, 14, 123)
            )
        );
    }

    #[test]
    fn test_str_with_nanosecond_timestamp() {
        let input = r#"{
            "time" : "2021-04-30T17:03:14.123456789+02:00",
            "temperature" : 25
        }"#;

        let output = ThinEdgeJson::from_str(input).unwrap();
        assert_eq!(
            output.timestamp,
            Some(
                FixedOffset::east(2 * 3600)
                    .ymd(2021, 4, 30)
                    .and_hms_nano(17, 3, 14, 123456789)
            )
        );
    }

    #[test]
    fn has_timestamp_returns_true_given_timestamp() {
        let input = r#"{
            "time" : "2021-04-30T17:03:14+02:00",
            "temperature" : 25
        }"#;
        let output = ThinEdgeJson::from_str(input).unwrap();
        assert_eq!(output.has_timestamp(), true);
    }

    #[test]
    fn has_timestamp_returns_false_given_no_timestamp() {
        let input = r#"{
            "temperature" : 25
        }"#;
        let output = ThinEdgeJson::from_str(input).unwrap();
        assert_eq!(output.has_timestamp(), false);
    }

    #[test]
    fn set_timestamp_adds_timestamp() {
        let input = r#"{
            "temperature" : 25
        }"#;
        let mut output = ThinEdgeJson::from_str(input).unwrap();
        let timestamp = FixedOffset::east(5 * 3600).ymd(2021, 4, 8).and_hms(0, 0, 0);
        output.set_timestamp(timestamp);
        assert_eq!(output.timestamp, Some(timestamp));
    }

    #[test]
    fn thin_edge_json_reject_arrays() {
        let input = r"[50,23]";

        let expected_error = r#"Invalid Thin Edge measurement: it cannot be an array: [50,23]"#;
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(string_value_thin_edge_json);

        let error = output.unwrap_err();
        assert_eq!(expected_output, error.to_string());
    }

    #[test]
    fn thin_edge_reject_deep_hierarchy() {
        let multi_level_hierarchy = r#"{
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
        let output = ThinEdgeJson::from_str(multi_level_hierarchy);
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
        let output = ThinEdgeJson::from_str(string_value_thin_edge_json);

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
        let output = ThinEdgeJson::from_str(string_value_thin_edge_json);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

        let error = output.unwrap_err();
        assert_eq!(expected_error, error.to_string());
    }

    #[test]
    fn thin_edge_json_reject_empty_record() {
        let input = "{}";

        let expected_error =
            "Empty Thin Edge measurement: it must contain at least one measurement";
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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
        let output = ThinEdgeJson::from_str(input);

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

    #[test]
    fn thin_edge_json_reject_invalid_key() {
        let input = r#"{
            "key with backslash: \\": 220
          }"#;

        let expected_error = "Invalid Thin Edge key: key with backslash: \\";
        let output = ThinEdgeJson::from_str(input);

        let error = output.unwrap_err();

        assert_eq!(expected_error, error.to_string());
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prefix_doesnt_crash(input in "\\PC*") {
            let _ = input_prefix(&input, 10);
        }
    }
}
