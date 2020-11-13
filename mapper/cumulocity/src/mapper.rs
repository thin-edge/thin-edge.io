use open_json::MeasurementRecord;
use core::fmt;

/// Convert a measurement record into a sequence of SmartRest messages
///
/// ```
/// let input = r#"{
///     "temperature": 23,
///     "battery": 99
/// }"#;
///
/// let record = MeasurementRecord::from_json(input).unwrap();
///
/// let smart_rest = into_smart_rest(&record).unwrap();
///
/// assert_eq!(smart_rest, vec![
///     "211,23".into(),
///     "212,99".into(),
/// ]);
/// ```
pub fn into_smart_rest(record: &MeasurementRecord) -> Result<Vec<String>, Error> {
    let mut messages = Vec::new();
    for (k,v) in record.measurements().iter() {
        if k == "temperature" {
            messages.push(format!("211,{}", v));
        }
        else if k == "battery" {
            messages.push(format!("212,{}", v));
        }
        else {
            return Err(Error::UnknownTemplate(k.clone()));
        }
    }
    Ok(messages)
}

/// Translation errors
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    UnknownTemplate(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::UnknownTemplate(ref t) => write!(f, "Unknown template '{}'", t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_temperature() {
        let input = r#"{"temperature": 23}"#;
        let expected= vec!["211,23".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn map_battery() {
        let input = r#"{"battery": 99}"#;
        let expected= vec!["212,99".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn map_record() {
        let input = r#"{"temperature": 23, "battery": 99}"#;
        let expected= vec!["211,23".into(), "212,99".into()];
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Ok(expected), into_smart_rest(&record))
    }

    #[test]
    fn unknown_template() {
        let input = r#"{"pressure": 20}"#;
        let record = MeasurementRecord::from_json(input).unwrap();
        assert_eq!(Err(Error::UnknownTemplate("pressure".into())), into_smart_rest(&record))
    }
}