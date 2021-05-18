use std::convert::{TryFrom, TryInto};

/// Represents a boolean type.
///
/// We need this newtype in order to implement `TryFrom<String>` and `TryInto<String>`.
/// The config_key macro uses query_string() and update_string().
/// Therefore, boolean needs to be converted from/to String.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct Flag(pub bool);

#[derive(thiserror::Error, Debug)]
#[error("Failed to parse flag: {input}. Supported values are: true, false")]
pub struct InvalidFlag {
    input: String,
}

impl TryFrom<String> for Flag {
    type Error = InvalidFlag;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        match input.as_str() {
            "true" => Ok(Flag(true)),
            "false" => Ok(Flag(false)),
            _ => Err(InvalidFlag { input }),
        }
    }
}

impl TryInto<String> for Flag {
    type Error = std::convert::Infallible;

    fn try_into(self) -> Result<String, Self::Error> {
        match self {
            Flag(true) => Ok(String::from("true")),
            Flag(false) => Ok(String::from("false")),
        }
    }
}

impl Into<bool> for Flag {
    fn into(self) -> bool {
        self.0
    }
}

impl Flag {
    pub fn is_set(&self) -> bool {
        self.0
    }
}

#[test]
fn convert_string_true_to_bool_true() {
    let input = "true".to_string();
    let output: bool = Flag::try_from(input).unwrap().into();
    assert_eq!(output, true);
}

#[test]
fn convert_string_false_to_bool_false() {
    let input = "false".to_string();
    let output: bool = Flag::try_from(input).unwrap().into();
    assert_eq!(output, false);
}

#[test]
fn return_error_for_unexpected_string_input() {
    let input = "unknown".to_string();
    assert!(Flag::try_from(input).is_err());
}

#[test]
fn convert_bool_true_to_string_true() {
    let input = true;
    let output: String = Flag::try_into(Flag(input)).unwrap();
    assert_eq!(output, "true");
}

#[test]
fn convert_bool_false_to_string_false() {
    let input = false;
    let output: String = Flag::try_into(Flag(input)).unwrap();
    assert_eq!(output, "false");
}
