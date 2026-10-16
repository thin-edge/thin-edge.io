use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use tedge_config_engine::AppendRemoveItem;

#[derive(Clone, Debug, PartialEq, Eq, facet::Facet)]
#[facet(proxy = DeserializeTime)]
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
        match input.parse::<u64>() {
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

impl AppendRemoveItem for SecondsOrHumanTime {
    fn append(_current: Option<Self>, new_value: Self) -> Option<Self> {
        Some(new_value)
    }

    fn remove(current: Option<Self>, remove_value: Self) -> Option<Self> {
        match current {
            Some(v) if v == remove_value => None,
            other => other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, facet::Facet)]
#[repr(u8)]
#[facet(untagged)]
enum DeserializeTime {
    Seconds(u64),
    MaybeHumanTime(String),
}

impl fmt::Display for DeserializeTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeTime::Seconds(secs) => fmt::Display::fmt(secs, f),
            DeserializeTime::MaybeHumanTime(input) => fmt::Display::fmt(input, f),
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

impl From<&SecondsOrHumanTime> for DeserializeTime {
    fn from(value: &SecondsOrHumanTime) -> Self {
        value.input.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_seconds() {
        let t: SecondsOrHumanTime = "1234".parse().unwrap();
        assert_eq!(t.duration(), Duration::from_secs(1234));
        assert_eq!(t.to_string(), "1234");
    }

    #[test]
    fn parses_human_time() {
        let t: SecondsOrHumanTime = "20 minutes 34s".parse().unwrap();
        assert_eq!(t.duration(), Duration::from_secs(20 * 60 + 34));
        assert_eq!(t.to_string(), "20 minutes 34s");
    }

    #[test]
    fn rejects_invalid_input() {
        assert!("not a duration".parse::<SecondsOrHumanTime>().is_err());
    }

    #[test]
    fn overflow_is_rejected() {
        assert!("18446744073709551616"
            .parse::<SecondsOrHumanTime>()
            .is_err());
    }
}
