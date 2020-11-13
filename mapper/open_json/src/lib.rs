//! Open-edge is a JSON schema to encode measurement records.
//!
//! ```
//! use open_json::MeasurementRecord;
//!
//! let input = r#"{
//!     "temperature": 23,
//!     "pressure": 220
//! }"#;
//!
//! let record = MeasurementRecord::from_json(input).unwrap();
//!
//! assert_eq!(record.measurements(), &vec![
//!     ("temperature".into(), 23.0),
//!     ("pressure".into(), 220.0),
//!]);
//! ```

use core::fmt;
use json::JsonValue;

/// A measurement record is a collection of measurements
/// each defined by a name and a numeric value.
#[derive(Debug)]
pub struct MeasurementRecord {
    measurements: Vec<(String,f64)>,
}

impl MeasurementRecord {
    /// Read a measurement record from a json input
    /// ```
    /// use open_json::MeasurementRecord;
    ///
    /// let input = r#"{
    ///     "temperature": 23,
    ///     "pressure": 220
    /// }"#;
    ///
    /// let record = MeasurementRecord::from_json(input).unwrap();
    ///
    /// assert_eq!(record.measurements(), &vec![
    ///     ("temperature".into(), 23.0),
    ///     ("pressure".into(), 220.0),
    ///]);
    /// ```
    pub fn from_json(input: &str) -> Result<MeasurementRecord, Error> {
        let json = json::parse(input).map_err(|err| Error::NotJson(err))?;
        match json {
            JsonValue::Object(obj) => MeasurementRecord::from_json_object(obj),
            _ => return Err(Error::NotAnObject),
        }
    }

    /// Read a measurement record from slice of bytes
    /// ```
    /// use open_json::MeasurementRecord;
    ///
    /// let input = b"{
    ///     \"temperature\": 23,
    ///     \"pressure\": 220
    /// }";
    ///
    /// let record = MeasurementRecord::from_bytes(input).unwrap();
    ///
    /// assert_eq!(record.measurements(), &vec![
    ///     ("temperature".into(), 23.0),
    ///     ("pressure".into(), 220.0),
    ///]);
    /// ```
    pub fn from_bytes(input: &[u8]) -> Result<MeasurementRecord, Error> {
        let input = std::str::from_utf8(input).map_err(|err| Error::NotUtf8(err))?;
        MeasurementRecord::from_json(input)
    }

    /// Build a measurement record from a json object
    fn from_json_object(obj: json::object::Object) -> Result<MeasurementRecord, Error> {
        let mut measurements = Vec::new();
        for (k, v) in obj.iter() {
            match v {
                JsonValue::Number(num) => {
                    let value: f64 = (*num).into();
                    measurements.push((k.into(), value));
                }
                _ => return Err(Error::NotANumber),
            }
        }
        Ok(MeasurementRecord { measurements })
    }

    pub fn measurements(&self) -> &Vec<(String,f64)> {
        &self.measurements
    }
}

impl fmt::Display for MeasurementRecord {

    /// Display a measurement record
    ///
    /// ```
    /// use open_json::MeasurementRecord;
    ///
    /// let input = r#"{
    ///     "temperature": 23,
    ///     "pressure": 220
    /// }"#;
    /// let record = MeasurementRecord::from_json(input).unwrap();
    ///
    /// let output = format!("{}", record);
    ///
    /// assert_eq!(output, r#"{"temperature": 23, "pressure": 220}"#);
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut sep = "";
        write!(f,"{{")?;
        for (k,v) in self.measurements.iter() {
            write!(f, "{}\"{}\": {}", sep, k, v)?;
            sep = ", "
        }
        write!(f,"}}")
    }
}

/// Parsing errors
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    NotUtf8(std::str::Utf8Error),
    NotJson(json::Error),
    NotAnObject,
    NotANumber,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::NotUtf8(ref err) => write!(f, "Utf8 error: {}", err),
            Error::NotJson(ref err) => write!(f, "Json format error: {}", err),
            Error::NotAnObject => write!(f, "A record of measurement is expected"),
            Error::NotANumber => write!(f, "Only scalar values are expected"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let input = r#"{"temperature": 23, "pressure": 220}"#;
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(record.measurements, vec![
            ("temperature".into(), 23.0),
            ("pressure".into(), 220.0),
        ]);
    }

    #[test]
    fn test_display() {
        let record = MeasurementRecord {
            measurements: vec![
                ("temperature".into(), 23.0),
                ("pressure".into(), 220.0),
            ]
        };

        assert_eq!(format!("{}", record), r#"{"temperature": 23, "pressure": 220}"#);
    }

    #[test]
    fn must_reject_non_json_input() {
        let input = r#"some non-json input"#;
        let error = MeasurementRecord::from_json(input).err().unwrap();
        assert_eq!(format!("{}", error), "Json format error: Unexpected character: s at (1:1)");
    }

    #[test]
    fn must_reject_non_object_input() {
        let input = r#"["temperature", 23, "pressure", 220]"#;
        let error = MeasurementRecord::from_json(input).err().unwrap();
        assert_eq!(error, Error::NotAnObject);
    }

    #[test]
    fn must_reject_non_numeric_measurement() {
        let input = r#"{"temperature": "hot"}"#;
        let error = MeasurementRecord::from_json(input).err().unwrap();
        assert_eq!(error, Error::NotANumber);
    }

    // See the [PropTest Book](https://altsysrq.github.io/proptest-book/intro.html)
    use proptest::prelude::*;

    proptest! {
        #[test] // Test the parser with arbitrary strings
        fn doesnt_crash(s in "\\PC*") {
            let _ = MeasurementRecord::from_json(&s);
        }

        #[test] // Test Open Edge json with arbitrary whitespaces
        #[ignore]
        fn parse_open_edge(s in r#"\s"[a-z]*"\s:\s[1-9][0-9]*\s"#) {
             let input = format!("{{{}}}", s); // adding curly braces around s
             MeasurementRecord::from_json(&input).unwrap();
        }

        #[test] // Test valid Open Edge json
        fn parse_valid_open_edge(s in r#"( *"\w" *: *[1-9][0-9]* *,){0,3} *"\w" *: *[1-9][0-9]* *"#) {
             let input = format!("{{{}}}", s); // adding curly braces around s
             MeasurementRecord::from_json(&input).unwrap();
        }
    }
}