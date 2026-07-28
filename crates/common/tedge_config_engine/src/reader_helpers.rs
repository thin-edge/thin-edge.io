//! Helpers called by `define_config!`-generated reader `build` methods.

use std::str::FromStr;

use facet::Facet;

use crate::defaults::config_get_with_defaults;
use crate::defaults::DefaultSpec;
use crate::defaults::DefaultsRegistry;
use crate::defaults::RootResolver;
use crate::reflect::ConfigError;
use crate::OptionalConfig;

/// Reads a required field (one with a concrete default) from the DTO, parsing it from its string representation
pub fn read_required<T: FromStr>(
    dto: &impl for<'a> Facet<'a>,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
) -> Result<T, ConfigError>
where
    T::Err: std::fmt::Display,
{
    let value = config_get_with_defaults(dto, key, defaults, root_resolver)?.ok_or_else(|| {
        ConfigError::ReflectError(format!(
            "Required config key '{key}' is not set and has no default"
        ))
    })?;
    value
        .parse()
        .map_err(|e: T::Err| ConfigError::ParseError(format!("Failed to parse '{key}': {e}")))
}

/// Reads an optional field (no guaranteed default) into an `OptionalConfig`
pub fn read_optional<T: FromStr>(
    dto: &impl for<'a> Facet<'a>,
    defaults: &DefaultsRegistry,
    root_resolver: RootResolver<'_>,
    key: &str,
    display_prefix: &str,
    profile: Option<&str>,
) -> Result<OptionalConfig<T>, ConfigError>
where
    T::Err: std::fmt::Display,
{
    let value = match config_get_with_defaults(dto, key, defaults, root_resolver) {
        Ok(Some(v)) => Some(v),
        Ok(None) => None,
        Err(ConfigError::ReflectError(_)) => None,
        Err(e) => return Err(e),
    };

    let profile_value = profile.map(str::to_owned);

    match value {
        Some(v) => {
            let display_key = dotted_key(display_prefix, key);
            let parsed: T = v.parse().map_err(|e: T::Err| {
                ConfigError::ParseError(format!(
                    "Failed to parse value for optional field '{key}': {e}"
                ))
            })?;
            Ok(OptionalConfig::present(parsed, display_key).with_profile(profile_value))
        }
        None => {
            let presentation = presentation_key(defaults, key);
            let display_key = dotted_key(display_prefix, presentation);
            Ok(OptionalConfig::empty(display_key).with_profile(profile_value))
        }
    }
}

fn presentation_key<'a>(defaults: &'a DefaultsRegistry, mut key: &'a str) -> &'a str {
    for _ in 0..10 {
        match defaults.get(key) {
            Some(DefaultSpec::FromOptionalKey(source)) => key = source,
            _ => break,
        }
    }
    key
}

fn dotted_key(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}
