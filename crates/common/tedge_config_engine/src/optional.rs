use facet::Facet;
use std::ops::Deref;

pub(crate) const OPTIONAL_CONFIG_TYPE_TAG: &str = "optional_config";

/// The value of an optional configuration (i.e. one without a default value)
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
#[facet(type_tag = "optional_config")]
pub enum OptionalConfig<T> {
    /// Equivalent to `Some(T)`
    Present {
        value: T,
        key: String,
        profile: Option<String>,
    },
    /// Equivalent to `None`, but stores the configuration key to create a
    /// better error message
    Empty {
        key: String,
        profile: Option<String>,
    },
}

/// An error indicating a configuration value was read but not set
#[derive(Debug)]
pub struct ConfigNotSet {
    pub key: String,
    pub profile: Option<String>,
}

impl std::fmt::Display for ConfigNotSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.profile {
            Some(profile) => write!(
                f,
                "A value for '{key}' is missing (profile '{profile}').\n    \
                 A value can be set with `tedge config set --profile {profile} {key} <value>`",
                key = self.key
            ),
            None => write!(
                f,
                "A value for '{key}' is missing.\n    \
                 A value can be set with `tedge config set {key} <value>`",
                key = self.key
            ),
        }
    }
}

impl std::error::Error for ConfigNotSet {}

impl<T> OptionalConfig<T> {
    /// Creates an [OptionalConfig::Present] with the provided value and key
    pub fn present(value: T, key: impl Into<String>) -> Self {
        Self::Present {
            value,
            key: key.into(),
            profile: None,
        }
    }

    /// Creates an [OptionalConfig::Empty] with the provided key
    pub fn empty(key: impl Into<String>) -> Self {
        Self::Empty {
            key: key.into(),
            profile: None,
        }
    }

    /// Attaches a profile to this config value
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        match &mut self {
            Self::Present { profile: p_ref, .. } | Self::Empty { profile: p_ref, .. } => {
                *p_ref = profile
            }
        }
        self
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
            Self::Empty { key, profile } => Err(ConfigNotSet {
                key: key.clone(),
                profile: profile.clone(),
            }),
        }
    }

    /// Returns the configuration key associated with this value
    pub fn key(&self) -> &str {
        match self {
            Self::Present { key, .. } => key,
            Self::Empty { key, .. } => key,
        }
    }

    /// Returns the profile associated with this value, if any
    pub fn profile(&self) -> Option<&str> {
        match self {
            Self::Present { profile, .. } | Self::Empty { profile, .. } => profile.as_deref(),
        }
    }

    /// Returns the key formatted for user-facing display, including profile
    pub fn display_key(&self) -> String {
        format_display_key(self.key(), self.profile())
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
            Self::Empty { key, .. } => Some(key),
            Self::Present { .. } => None,
        }
    }

    /// Converts from `&OptionalConfig<T>` to `OptionalConfig<&T>`
    pub fn as_ref(&self) -> OptionalConfig<&T> {
        match self {
            Self::Present {
                value,
                key,
                profile,
            } => OptionalConfig::Present {
                value,
                key: key.clone(),
                profile: profile.clone(),
            },
            Self::Empty { key, profile } => OptionalConfig::Empty {
                key: key.clone(),
                profile: profile.clone(),
            },
        }
    }

    /// Converts from `&OptionalConfig<T>` to `OptionalConfig<&T::Target>`
    pub fn as_deref(&self) -> OptionalConfig<&<T as Deref>::Target>
    where
        T: Deref,
    {
        match self {
            Self::Present {
                value,
                key,
                profile,
            } => OptionalConfig::Present {
                value,
                key: key.clone(),
                profile: profile.clone(),
            },
            Self::Empty { key, profile } => OptionalConfig::Empty {
                key: key.clone(),
                profile: profile.clone(),
            },
        }
    }

    /// Maps the contained value, preserving key and profile
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> OptionalConfig<U> {
        match self {
            Self::Present {
                value,
                key,
                profile,
            } => OptionalConfig::Present {
                value: f(value),
                key,
                profile,
            },
            Self::Empty { key, profile } => OptionalConfig::Empty { key, profile },
        }
    }
}

pub(crate) fn format_display_key(key: &str, profile: Option<&str>) -> String {
    match profile {
        Some(profile) => format!("{key} (profile '{profile}')"),
        None => key.to_owned(),
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
    fn unset_value_error_includes_profile() {
        let config =
            OptionalConfig::<String>::empty("c8y.url").with_profile(Some("staging".into()));
        let err = config.or_config_not_set().unwrap_err();
        assert_eq!(
            err.to_string(),
            "A value for 'c8y.url' is missing (profile 'staging').\n    \
             A value can be set with `tedge config set --profile staging c8y.url <value>`"
        );
    }

    #[test]
    fn display_key_includes_profile() {
        let config = OptionalConfig::present(1, "c8y.url").with_profile(Some("staging".into()));
        assert_eq!(config.key(), "c8y.url");
        assert_eq!(config.display_key(), "c8y.url (profile 'staging')");
    }

    #[test]
    fn display_key_without_profile() {
        let config = OptionalConfig::present(1, "c8y.url");
        assert_eq!(config.display_key(), "c8y.url");
    }

    #[test]
    fn map_preserves_key_and_profile() {
        let config =
            OptionalConfig::present(8883u16, "mqtt.port").with_profile(Some("staging".into()));
        let mapped = config.map(|port| port.to_string());
        assert_eq!(mapped.or_none(), Some(&"8883".to_string()));
        assert_eq!(mapped.key(), "mqtt.port");
        assert_eq!(mapped.profile(), Some("staging"));
    }
}
