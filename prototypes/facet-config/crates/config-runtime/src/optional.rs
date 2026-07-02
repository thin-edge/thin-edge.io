use facet::Facet;
use std::ops::Deref;

pub(crate) const OPTIONAL_CONFIG_TYPE_TAG: &str = "optional_config";

/// The value of an optional configuration (i.e. one without a default value)
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
#[facet(type_tag = "optional_config")]
pub enum OptionalConfig<T> {
    /// Equivalent to `Some(T)`
    Present { value: T, key: String },
    /// Equivalent to `None`, but stores the configuration key to create a
    /// better error message
    Empty { key: String },
}

/// An error indicating a configuration value was read but not set
#[derive(Debug, thiserror::Error)]
#[error(
    r#"A value for '{key}' is missing.
    A value can be set with `tedge config set {key} <value>`"#
)]
pub struct ConfigNotSet {
    pub key: String,
}

impl<T> OptionalConfig<T> {
    /// Creates an [OptionalConfig::Present] with the provided value and key
    pub fn present(value: T, key: impl Into<String>) -> Self {
        Self::Present {
            value,
            key: key.into(),
        }
    }

    /// Creates an [OptionalConfig::Empty] with the provided key
    pub fn empty(key: impl Into<String>) -> Self {
        Self::Empty { key: key.into() }
    }

    /// Converts the value to an [Option], discarding the key
    pub fn or_none(&self) -> Option<&T> {
        match self {
            Self::Present { value, .. } => Some(value),
            Self::Empty { .. } => None,
        }
    }

    /// Returns the value, or a [ConfigNotSet] error naming the unset key
    pub fn or_config_not_set(&self) -> Result<&T, ConfigNotSet> {
        match self {
            Self::Present { value, .. } => Ok(value),
            Self::Empty { key } => Err(ConfigNotSet { key: key.clone() }),
        }
    }

    /// Returns the configuration key associated with this value
    pub fn key(&self) -> &str {
        match self {
            Self::Present { key, .. } => key,
            Self::Empty { key } => key,
        }
    }

    /// Returns the key if a value is set
    pub fn key_if_present(&self) -> Option<&str> {
        match self {
            Self::Present { key, .. } => Some(key),
            Self::Empty { .. } => None,
        }
    }

    /// Returns the key if no value is set
    pub fn key_if_empty(&self) -> Option<&str> {
        match self {
            Self::Empty { key } => Some(key),
            Self::Present { .. } => None,
        }
    }

    /// Converts from `&OptionalConfig<T>` to `OptionalConfig<&T>`
    pub fn as_ref(&self) -> OptionalConfig<&T> {
        match self {
            Self::Present { value, key } => OptionalConfig::Present {
                value,
                key: key.clone(),
            },
            Self::Empty { key } => OptionalConfig::Empty { key: key.clone() },
        }
    }

    /// Converts from `&OptionalConfig<T>` to `OptionalConfig<&T::Target>`
    pub fn as_deref(&self) -> OptionalConfig<&<T as Deref>::Target>
    where
        T: Deref,
    {
        match self {
            Self::Present { value, key } => OptionalConfig::Present {
                value,
                key: key.clone(),
            },
            Self::Empty { key } => OptionalConfig::Empty { key: key.clone() },
        }
    }

    /// Maps the contained value, preserving the key
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> OptionalConfig<U> {
        match self {
            Self::Present { value, key } => OptionalConfig::Present {
                value: f(value),
                key,
            },
            Self::Empty { key } => OptionalConfig::Empty { key },
        }
    }
}

impl<T> From<OptionalConfig<T>> for Option<T> {
    fn from(value: OptionalConfig<T>) -> Self {
        match value {
            OptionalConfig::Present { value, .. } => Some(value),
            OptionalConfig::Empty { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_value_is_returned_by_or_none() {
        let config = OptionalConfig::present(1883u16, "mqtt.port");
        assert_eq!(config.or_none(), Some(&1883));
    }

    #[test]
    fn empty_value_converts_to_none() {
        let config = OptionalConfig::<u16>::empty("mqtt.port");
        assert_eq!(config.or_none(), None);
    }

    #[test]
    fn unset_value_error_names_the_key() {
        let config = OptionalConfig::<String>::empty("device.id");
        let err = config.or_config_not_set().unwrap_err();
        assert_eq!(
            err.to_string(),
            "A value for 'device.id' is missing.\n    A value can be set with `tedge config set device.id <value>`"
        );
    }

    #[test]
    fn key_is_available_for_both_variants() {
        assert_eq!(OptionalConfig::present(1, "some.key").key(), "some.key");
        assert_eq!(OptionalConfig::<i32>::empty("some.key").key(), "some.key");
    }

    #[test]
    fn map_preserves_the_key() {
        let config = OptionalConfig::present(8883u16, "mqtt.port");
        let mapped = config.map(|port| port.to_string());
        assert_eq!(mapped.or_none(), Some(&"8883".to_string()));
        assert_eq!(mapped.key(), "mqtt.port");
    }
}
