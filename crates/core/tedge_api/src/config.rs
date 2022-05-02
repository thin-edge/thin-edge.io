use std::collections::HashMap;

use serde::Serialize;

use crate::message::MessageType;

/// Generic config that represents what kind of config a plugin wishes to accept
#[derive(Debug, Serialize)]
pub struct Config {
    kind: ConfigKind,
    doc: Option<String>,
}

impl Config {
    /// Construct a new generic config explanation
    #[must_use]
    pub fn new(kind: ConfigKind, doc: Option<String>) -> Self {
        Self { kind, doc }
    }

    /// Construct a new generic config explanation from a single kind
    ///
    /// This leaves the documentation set to [`None`]
    #[must_use]
    pub fn from_kind(kind: ConfigKind) -> Self {
        Self { kind, doc: None }
    }

    /// Get a reference to the config's documentation.
    #[must_use]
    pub fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
    }

    /// Get a reference to the config's kind.
    #[must_use]
    pub fn kind(&self) -> &ConfigKind {
        &self.kind
    }

    /// Set or replace the documentation of this [`Config`]
    #[must_use]
    pub fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }
}

/// The specific kind a [`Config`] represents
#[derive(Debug, Serialize)]
pub enum ConfigKind {
    /// Config represents a boolean `true`/`false`
    Bool,

    /// Config represents an integer `1, 10, 200, 10_000, ...`
    ///
    /// # Note
    ///
    /// The maximum value that can be represented is between [`i64::MIN`] and [`i64::MAX`]
    Integer,

    /// Config represents a floating point value `1.0, 20.235, 3.1419`
    ///
    /// # Note
    /// Integers are also accepted and converted to their floating point variant
    ///
    /// The maximum value that can be represented is between [`f64::MIN`] and [`f64::MAX`]
    Float,

    /// Config represents a string
    String,

    /// Config represents an array of values of the given [`ConfigKind`]
    Array(Box<Config>),

    /// Config represents a map of different configurations
    Struct(HashMap<String, Config>),

    /// Config represents a hashmap of named configurations of the same type
    ///
    /// # Note
    ///
    /// The key is always a [`String`] so this only holds the value config
    HashMap(Box<Config>),
}

/// Turn a plugin configuration into a [`Config`] object
///
/// Plugin authors are expected to implement this for their configurations to give users
pub trait AsConfig {
    /// Get a [`Config`] object from the type
    fn as_config() -> Config;
}

impl<T: AsConfig> AsConfig for Vec<T> {
    fn as_config() -> Config {
        Config::from_kind(ConfigKind::Array(Box::new(T::as_config())))
    }
}

impl<V: AsConfig> AsConfig for HashMap<String, V> {
    fn as_config() -> Config {
        Config::from_kind(ConfigKind::HashMap(Box::new(V::as_config())))
    }
}

macro_rules! impl_config_kind {
    ($kind:expr => $($name:ty),+) => {
        $(
            impl AsConfig for $name {
                fn as_config() -> Config {
                    Config::from_kind($kind)
                }
            }
        )+
    };
}

impl_config_kind!(ConfigKind::Integer => u64, i64);
impl_config_kind!(ConfigKind::Float => f64);
impl_config_kind!(ConfigKind::Bool => bool);
impl_config_kind!(ConfigKind::String => String);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::config::{AsConfig, Config, ConfigKind};

    #[test]
    fn verify_correct_config_kinds() {
        assert!(matches!(
            Vec::<f64>::as_config(),
            Config {
                doc: None,
                kind: ConfigKind::Array(x)
            } if matches!(x.kind(), ConfigKind::Float)
        ));

        let complex_config = HashMap::<String, Vec<HashMap<String, String>>>::as_config();
        println!("Complex config: {:#?}", complex_config);

        assert!(
            matches!(complex_config.kind(), ConfigKind::HashMap(map) if matches!(map.kind(), ConfigKind::Array(arr) if matches!(arr.kind(), ConfigKind::HashMap(inner_map) if matches!(inner_map.kind(), ConfigKind::String))))
        );
    }
}
