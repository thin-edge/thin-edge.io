//! Runtime support for reflection-based thin-edge configuration.
//!
//! This crate is the glue between generated Facet DTOs and the string-oriented
//! config surface used by CLI commands, environment variables, and federated
//! config files.
//!
//! The root config is the main `tedge.toml`; mapper configs are per-cloud
//! connector configs mounted below keys like `mappers.c8y.*`.

mod append_remove;
mod defaults;
mod host_port;
mod manager;
mod optional;
mod reader;
mod reflect;
mod templates_set;
pub mod type_action;

/// Combines root config and mapper configs behind one key space.
pub mod federated;
/// Object-safe config operations used by mounted config sources.
pub mod ops;

pub use append_remove::{register_append_remove, AppendRemoveItem, AppendRemoveRegistry};
pub use defaults::{DefaultSpec, DefaultsRegistry, EnvOverrides, FieldDefault, RootResolver};
pub use host_port::{HostPort, ParseHostPortError, HTTPS_PORT};
pub use manager::ConfigManager;
pub use optional::{ConfigNotSet, OptionalConfig};
pub use reader::build_reader;
pub use reader::build_reader_at;
pub use reflect::{
    config_get, config_set, config_unset, list_key_entries, list_keys, ConfigError, DeprecatedKey,
    KeyAliases, KeyEntry, ReadOnlyKeys,
};
pub use templates_set::TemplatesSet;
