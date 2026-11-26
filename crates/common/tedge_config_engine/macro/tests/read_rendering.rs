//! Exercises how `ConfigOps::read` shows a defaulted value: as the key's own
//! type would show it, whatever key or schema default the value came from

use std::fmt;
use std::str::FromStr;
use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::AppendRemoveItem;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_utils::paths::TedgePaths;

mod schema {
    use super::Host;
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        Rendering {
            /// A host name as the user wrote it
            url: String,

            /// A host with the port the key's type adds
            #[tedge_config(default(from_optional_key = "url"))]
            host: Host,

            /// A host whose schema default has no port
            #[tedge_config(default(value = "example.com"))]
            fixed_host: Host,
        }
    }
}

#[test]
fn value_defaulted_from_another_key_reads_as_this_keys_type() {
    let mut source = source();
    set(&mut source, "url", "example.com");

    assert_eq!(
        source.read("host", None).unwrap().as_deref(),
        Some("example.com:443")
    );
}

#[test]
fn schema_default_reads_as_this_keys_type() {
    let source = source();

    assert_eq!(
        source.read("fixed_host", None).unwrap().as_deref(),
        Some("example.com:443")
    );
}

#[test]
fn explicit_value_reads_as_stored() {
    let mut source = source();
    set(&mut source, "host", "example.com:8443");

    assert_eq!(
        source.read("host", None).unwrap().as_deref(),
        Some("example.com:8443")
    );
}

#[test]
fn fallback_value_that_is_invalid_for_this_key_names_both_keys() {
    let mut source = source();
    set(&mut source, "url", "not a host");

    let err = source.read("host", None).unwrap_err();

    let ConfigError::InvalidValue {
        key, source_key, ..
    } = &err
    else {
        panic!("expected an invalid value error, got {err:?}");
    };
    assert_eq!((key.as_str(), source_key.as_str()), ("host", "url"));
}

fn source() -> TypedConfigOps<schema::RenderingConfigDto> {
    let manager =
        ConfigManager::from_schema::<schema::RenderingConfig>(std::path::Path::new("/etc/tedge"));
    let file = TedgePaths::from_root_with_defaults("/nonexistent/tedge", "", "")
        .file("config.toml")
        .unwrap();
    TypedConfigOps::empty(manager, file)
}

fn set(source: &mut TypedConfigOps<schema::RenderingConfigDto>, key: &str, value: &str) {
    let manager =
        ConfigManager::from_schema::<schema::RenderingConfig>(std::path::Path::new("/etc/tedge"));
    manager.set(source.dto_mut(), key, value).unwrap();
}

/// A host and a port, defaulting to 443 when the input has none
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(proxy = String)]
pub struct Host {
    name: String,
    port: u16,
}

impl FromStr for Host {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(' ') {
            return Err(format!("'{s}' is not a host"));
        }
        match s.split_once(':') {
            Some((name, port)) => Ok(Self {
                name: name.to_owned(),
                port: port.parse().map_err(|e| format!("{e}"))?,
            }),
            None => Ok(Self {
                name: s.to_owned(),
                port: 443,
            }),
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.name, self.port)
    }
}

impl TryFrom<String> for Host {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<&Host> for String {
    fn from(value: &Host) -> Self {
        value.to_string()
    }
}

impl AppendRemoveItem for Host {
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
