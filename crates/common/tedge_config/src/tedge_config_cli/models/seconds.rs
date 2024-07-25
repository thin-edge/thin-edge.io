use std::fmt;
use std::fmt::Debug;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(into = "String", try_from = "DeserializeTime")]
pub struct SecondsOrHumanTime {
    duration: Duration,
    input: DeserializeTime,
}

impl From<SecondsOrHumanTime> for String {
    fn from(value: SecondsOrHumanTime) -> Self {
        value.to_string()
    }
}

impl FromStr for SecondsOrHumanTime {
    type Err = humantime::DurationError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let seconds = input.parse::<u64>();

        match seconds {
            Ok(seconds) => Ok(Self {
                duration: Duration::from_secs(seconds),
                input: DeserializeTime::Seconds(seconds),
            }),
            Err(_) => humantime::parse_duration(input).map(|duration| Self {
                duration,
                input: DeserializeTime::MaybeHumanTime(input.to_owned()),
            }),
        }
    }
}

impl fmt::Display for SecondsOrHumanTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.input, f)
    }
}

impl SecondsOrHumanTime {
    pub fn duration(&self) -> Duration {
        self.duration
    }
}

impl doku::Document for SecondsOrHumanTime {
    fn ty() -> doku::Type {
        String::ty()
    }
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(untagged)]
enum DeserializeTime {
    Seconds(u64),
    MaybeHumanTime(String),
}

impl fmt::Display for DeserializeTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeTime::Seconds(secs) => fmt::Display::fmt(&secs, f),
            DeserializeTime::MaybeHumanTime(input) => fmt::Display::fmt(&input, f),
        }
    }
}

impl TryFrom<DeserializeTime> for SecondsOrHumanTime {
    type Error = humantime::DurationError;
    fn try_from(value: DeserializeTime) -> Result<Self, Self::Error> {
        match value {
            DeserializeTime::Seconds(secs) => Ok(Self {
                duration: Duration::from_secs(secs),
                input: value,
            }),
            DeserializeTime::MaybeHumanTime(human) => human.parse(),
        }
    }
}

#[test]
fn conversion_from_valid_seconds_succeeds() {
    assert_eq!(
        "1234".parse::<SecondsOrHumanTime>().unwrap(),
        SecondsOrHumanTime {
            duration: Duration::from_secs(1234),
            input: DeserializeTime::Seconds(1234),
        }
    );
}

#[test]
fn conversion_from_valid_humantime_succeeds() {
    assert_eq!(
        "1 hour".parse::<SecondsOrHumanTime>().unwrap(),
        SecondsOrHumanTime {
            duration: Duration::from_secs(3600),
            input: DeserializeTime::MaybeHumanTime("1 hour".into()),
        }
    );
}

#[test]
fn conversion_from_longer_integer_fails() {
    assert_eq!(
        "18446744073709551616"
            .parse::<SecondsOrHumanTime>()
            .unwrap_err()
            .to_string(),
        "number is too large"
    );
}

#[test]
fn display_implementation_preserves_format_of_seconds() {
    assert_eq!(
        "1234".parse::<SecondsOrHumanTime>().unwrap().to_string(),
        "1234"
    );
}

#[test]
fn display_implementation_preserves_format_of_humantime() {
    assert_eq!(
        "20 minutes 34s"
            .parse::<SecondsOrHumanTime>()
            .unwrap()
            .to_string(),
        "20 minutes 34s"
    );
}
