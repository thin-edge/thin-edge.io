use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

#[derive(
    Copy, Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq, doku::Document,
)]
#[serde(transparent)]
pub struct Seconds(pub(crate) u64);

#[derive(thiserror::Error, Debug)]
#[error("Invalid seconds number: '{input}'.")]
pub struct InvalidSecondsNumber {
    input: String,
}

impl TryFrom<String> for Seconds {
    type Error = InvalidSecondsNumber;

    fn try_from(input: String) -> Result<Self, Self::Error> {
        input
            .as_str()
            .parse::<u64>()
            .map_err(|_| InvalidSecondsNumber { input })
            .map(Seconds)
    }
}

impl TryInto<String> for Seconds {
    type Error = std::convert::Infallible;

    fn try_into(self) -> Result<String, Self::Error> {
        Ok(format!("{}", self.0))
    }
}

impl FromStr for Seconds {
    type Err = <u64 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Self)
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Seconds {
    pub fn duration(self) -> Duration {
        Duration::from_secs(self.0)
    }
}

impl From<Seconds> for u64 {
    fn from(val: Seconds) -> Self {
        val.0
    }
}

impl From<u64> for Seconds {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

#[cfg(test)]
use assert_matches::*;
#[test]
fn conversion_from_valid_seconds_succeeds() {
    assert_matches!(Seconds::try_from("1234".to_string()), Ok(Seconds(1234)));
}

#[test]
fn conversion_from_longer_integer_fails() {
    assert_matches!(
        Seconds::try_from("18446744073709551616".to_string()),
        Err(InvalidSecondsNumber { .. })
    );
}

#[test]
fn conversion_from_seconds_to_string() {
    assert_matches!(TryInto::<String>::try_into(Seconds(1234)), Ok(seconds_str) if seconds_str == "1234");
}
