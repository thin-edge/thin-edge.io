//! Handling for optional configuration values
//!
//! This module provides types used to represent the presence or absence of
//! values, but with the addition of metadata (such as the relevant
//! configuration key) to aid in producing informative error messages.

#[derive(serde::Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "Option<T>", bound = "T: Clone + serde::Serialize")]
/// The value for an optional configuration (i.e. one without a default value)
pub enum OptionalConfig<T> {
    /// Equivalent to `Some(T)`
    Present { value: T, key: &'static str },
    /// Equivalent to `None`, but stores the configuration key to create a
    /// better error message
    Empty(&'static str),
}

impl<T> From<OptionalConfig<T>> for Option<T> {
    fn from(value: OptionalConfig<T>) -> Self {
        match value {
            OptionalConfig::Present { value, .. } => Some(value),
            OptionalConfig::Empty(_key_name) => None,
        }
    }
}

#[derive(thiserror::Error, Debug)]
#[error(
    r#"A value for '{key}' is missing.\n\
    A value can be set with `tedge config set {key} <value>`"#
)]
pub struct ConfigNotSet {
    key: &'static str,
}

impl<T> OptionalConfig<T> {
    pub fn or_none(&self) -> Option<&T> {
        match self {
            Self::Present { value, .. } => Some(value),
            Self::Empty(_) => None,
        }
    }

    pub fn or_config_not_set(&self) -> Result<&T, ConfigNotSet> {
        match self {
            Self::Present { value, .. } => Ok(value),
            Self::Empty(key) => Err(ConfigNotSet { key }),
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Present { key, .. } => key,
            Self::Empty(key) => key,
        }
    }

    pub fn key_if_present(&self) -> Option<&'static str> {
        match self {
            Self::Present { key, .. } => Some(key),
            Self::Empty(..) => None,
        }
    }

    pub fn key_if_empty(&self) -> Option<&'static str> {
        match self {
            Self::Empty(key) => Some(key),
            Self::Present { .. } => None,
        }
    }

    pub fn as_ref(&self) -> OptionalConfig<&T> {
        match self {
            Self::Present { value, key } => OptionalConfig::Present { value, key },
            Self::Empty(key) => OptionalConfig::Empty(key),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> OptionalConfig<U> {
        match self {
            Self::Present { value, key } => OptionalConfig::Present {
                value: f(value),
                key,
            },
            Self::Empty(key) => OptionalConfig::Empty(key),
        }
    }
}

impl<T: doku::Document> doku::Document for OptionalConfig<T> {
    fn ty() -> doku::Type {
        Option::<T>::ty()
    }
}
