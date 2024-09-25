//! Handling for optional configuration values
//!
//! This module provides types used to represent the presence or absence of
//! values, but with the addition of metadata (such as the relevant
//! configuration key) to aid in producing informative error messages.

use std::borrow::Cow;
use std::ops::Deref;

#[derive(serde::Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(into = "Option<T>", bound = "T: Clone + serde::Serialize")]
/// The value for an optional configuration (i.e. one without a default value)
///
/// ```
/// use tedge_config_macros::*;
///
/// assert_eq!(
///     OptionalConfig::Present { value: "test", key: "device.type" }.or_none(),
///     Some(&"test"),
/// );
/// ```
pub enum OptionalConfig<T> {
    /// Equivalent to `Some(T)`
    Present { value: T, key: Cow<'static, str> },
    /// Equivalent to `None`, but stores the configuration key to create a
    /// better error message
    Empty(Cow<'static, str>),
}

impl<T> OptionalConfig<T> {
    pub fn present(value: T, key: impl Into<Cow<'static, str>>) -> Self {
        Self::Present {
            value,
            key: key.into(),
        }
    }

    pub fn empty(key: impl Into<Cow<'static, str>>) -> Self {
        Self::Empty(key.into())
    }
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
    r#"A value for '{key}' is missing.
    A value can be set with `tedge config set {key} <value>`"#
)]
/// A descriptive error for missing configurations
///
/// When a configuration is missing, it can be converted to this via
/// [OptionalConfig::or_config_not_set], and this will convert to a descriptive
/// error message telling the user which key to set.
pub struct ConfigNotSet {
    key: Cow<'static, str>,
}

impl<T> OptionalConfig<T> {
    /// Converts the value to an [Option]
    ///
    /// ```
    /// use tedge_config_macros::*;
    ///
    /// assert_eq!(
    ///     OptionalConfig::Present { value: "test", key: "device.type" }.or_none(),
    ///     Some(&"test"),
    /// );
    ///
    /// assert_eq!(OptionalConfig::Empty::<&str>("device.type").or_none(), None);
    /// ```
    pub fn or_none(&self) -> Option<&T> {
        match self {
            Self::Present { value, .. } => Some(value),
            Self::Empty(_) => None,
        }
    }

    /// Converts the value to a [Result] with an error that contains the missing
    /// key name
    pub fn or_config_not_set(&self) -> Result<&T, ConfigNotSet> {
        match self {
            Self::Present { value, .. } => Ok(value),
            Self::Empty(key) => Err(ConfigNotSet { key: key.clone() }),
        }
    }

    pub fn as_deref(&self) -> OptionalConfig<&<T as Deref>::Target>
    where
        T: Deref,
    {
        match self {
            Self::Present { ref value, key } => OptionalConfig::Present {
                value: value.deref(),
                key: key.clone(),
            },
            Self::Empty(key) => OptionalConfig::Empty(key.clone()),
        }
    }

    pub fn key(&self) -> &Cow<'static, str> {
        match self {
            Self::Present { key, .. } => key,
            Self::Empty(key) => key,
        }
    }

    pub fn key_if_present(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Present { key, .. } => Some(key.clone()),
            Self::Empty(..) => None,
        }
    }

    pub fn key_if_empty(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Empty(key) => Some(key.clone()),
            Self::Present { .. } => None,
        }
    }

    pub fn as_ref(&self) -> OptionalConfig<&T> {
        match self {
            Self::Present { value, key } => OptionalConfig::Present {
                value,
                key: key.clone(),
            },
            Self::Empty(key) => OptionalConfig::Empty(key.clone()),
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
