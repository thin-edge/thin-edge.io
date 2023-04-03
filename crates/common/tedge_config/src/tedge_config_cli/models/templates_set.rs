use std::convert::TryInto;

/// Represents a set of smartrest templates.
///
/// New type to add conversion methods and deduplicate provided templates.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
#[serde(from = "FromTomlOrCli")]
pub struct TemplatesSet(pub Vec<String>);

#[derive(serde::Deserialize)]
#[serde(from = "String")]
struct CommaDelimited(Vec<String>);

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum FromTomlOrCli {
    Toml(Vec<String>),
    Cli(CommaDelimited),
}

impl From<String> for CommaDelimited {
    fn from(value: String) -> Self {
        Self(
            value
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    }
}

impl From<FromTomlOrCli> for TemplatesSet {
    fn from(value: FromTomlOrCli) -> Self {
        match value {
            FromTomlOrCli::Toml(entries) => Self(entries),
            FromTomlOrCli::Cli(CommaDelimited(entries)) => Self(entries),
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error("TemplateSet to String conversion failed: {0:?}")]
pub struct TemplatesSetToStringConversionFailure(String);

impl TryFrom<Vec<String>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
        Ok(TemplatesSet(value))
    }
}

impl TryFrom<Vec<&str>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_from(value: Vec<&str>) -> Result<Self, Self::Error> {
        Ok(TemplatesSet(
            value.into_iter().map(|s| s.into()).collect::<Vec<String>>(),
        ))
    }
}

impl TryInto<Vec<String>> for TemplatesSet {
    type Error = TemplatesSetToStringConversionFailure;

    fn try_into(self) -> Result<Vec<String>, TemplatesSetToStringConversionFailure> {
        Ok(self.0)
    }
}

impl From<TemplatesSet> for String {
    fn from(val: TemplatesSet) -> Self {
        val.to_string()
    }
}

impl From<String> for TemplatesSet {
    fn from(val: String) -> Self {
        let strings = val.split(',').map(|ss| ss.into()).collect();
        TemplatesSet(strings)
    }
}

impl std::fmt::Display for TemplatesSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
