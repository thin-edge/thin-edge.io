//! Open-edge is a JSON schema to encode a measurement records.
//!
//!

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
    pub fn from_json(input: &str) -> Result<MeasurementRecord, Error> {
        let json = json::parse(input).map_err(|err| Error::NotJson(err))?;
        match json {
            JsonValue::Object(obj) => MeasurementRecord::from_json_object(obj),
            _ => return Err(Error::NotAnObject),
        }
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
}

impl fmt::Display for MeasurementRecord {
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
    NotJson(json::Error),
    NotAnObject,
    NotANumber,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
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
}