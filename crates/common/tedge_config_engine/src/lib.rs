//! Runtime support for reflection-based thin-edge configuration.
//!
//! This crate is the glue between generated Facet DTOs and the string-oriented
//! config surface used by CLI commands, environment variables, and federated
//! config files.
//!
//! The root config is the main `tedge.toml`; mapper configs are per-cloud
//! connector configs mounted under prefixes like `mappers.c8y`.

mod append_remove;
pub mod attrs;
mod defaults;
mod manager;
mod optional;
mod reader;
mod reflect;
mod schema;
pub mod type_action;

/// Combines root config and mapper configs behind one key space.
pub mod federated;
/// Object-safe config operations used by mounted config sources.
pub mod ops;

pub use append_remove::register_append_remove;
pub use append_remove::AppendRemoveItem;
pub use append_remove::AppendRemoveRegistry;
pub use defaults::derive_to_string;
pub use defaults::validate_root_dependencies;
pub use defaults::DefaultSpec;
pub use defaults::DefaultsRegistry;
pub use defaults::DeriveFn;
pub use defaults::EnvOverrides;
pub use defaults::FieldDefault;
pub use defaults::RootDependency;
pub use defaults::RootResolver;
pub use manager::ConfigManager;
pub use optional::ConfigNotSet;
pub use optional::OptionalConfig;
pub use reader::build_reader;
pub use reader::build_reader_at;
pub use reflect::check_read_only;
pub use reflect::config_get;
pub use reflect::config_set;
pub use reflect::config_unset;
pub use reflect::list_key_entries;
pub use reflect::list_keys;
pub use reflect::ConfigError;
pub use reflect::DeprecatedKey;
pub use reflect::KeyAliases;
pub use reflect::KeyEntry;
pub use schema::prefix_defaults;
pub use schema::ConfigSchema;
