use facet::Facet;

use crate::append_remove::AppendRemoveRegistry;
use crate::defaults::{config_get_with_defaults, DefaultsRegistry, EnvOverrides};
use crate::reader::build_reader_at;
use crate::reflect::{
    config_add, config_get, config_remove, config_set, config_unset, list_key_entries, list_keys,
    ConfigError, KeyAliases, KeyEntry, ReadOnlyKeys,
};

/// Facade over the config reflection/defaults/reader subsystems.
///
/// Holds the registries (defaults, append-remove rules, read-only keys, aliases)
/// and exposes high-level operations on any DTO that implements `Facet`.
pub struct ConfigManager {
    defaults: DefaultsRegistry,
    append_remove: AppendRemoveRegistry,
    read_only: ReadOnlyKeys,
    aliases: KeyAliases,
    examples: std::collections::HashMap<&'static str, &'static [&'static str]>,
    env_prefix: Option<String>,
}

impl ConfigManager {
    /// Creates a manager from the registries generated for a config schema.
    pub fn new(
        defaults: DefaultsRegistry,
        append_remove: AppendRemoveRegistry,
        read_only: ReadOnlyKeys,
        aliases: KeyAliases,
        examples: std::collections::HashMap<&'static str, &'static [&'static str]>,
    ) -> Self {
        Self {
            defaults,
            append_remove,
            read_only,
            aliases,
            examples,
            env_prefix: None,
        }
    }

    /// Sets the environment variable prefix used by `apply_env`.
    /// e.g. prefix `"TEDGE_"` means `TEDGE_DEVICE_ID` maps to key `device.id`.
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = Some(prefix.into());
        self
    }

    /// Returns the explicitly-set value for `key`, ignoring defaults.
    pub fn get<T: for<'a> Facet<'a>>(
        &self,
        dto: &T,
        key: &str,
    ) -> Result<Option<String>, ConfigError> {
        config_get(dto, key)
    }

    /// Returns the effective value for `key`, falling back to defaults.
    pub fn read<T: for<'a> Facet<'a>>(
        &self,
        dto: &T,
        key: &str,
    ) -> Result<Option<String>, ConfigError> {
        config_get_with_defaults(dto, key, &self.defaults, None)
    }

    /// Sets `key` to `value` in the DTO (parses and validates).
    pub fn set<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        config_set(dto, key, value)
    }

    /// Clears `key` in the DTO (resets to None/empty).
    pub fn unset<T: for<'a> Facet<'a>>(&self, dto: &mut T, key: &str) -> Result<(), ConfigError> {
        config_unset(dto, key)
    }

    /// Appends `value` to a list-typed `key`.
    pub fn add<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        config_add(dto, key, value, &self.append_remove)
    }

    /// Removes `value` from a list-typed `key`.
    pub fn remove<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        config_remove(dto, key, value, &self.append_remove)
    }

    /// Returns an error if `key` is marked read-only.
    pub fn check_read_only(&self, key: &str) -> Result<(), ConfigError> {
        self.read_only.check(key)
    }

    /// Resolves a potentially-aliased key to the canonical key.
    /// Returns (canonical_key, Some(deprecation_message)) if the key was aliased.
    pub fn resolve_key(&self, key: &str) -> (String, Option<&'static str>) {
        self.aliases.resolve(key)
    }

    /// Lists all dot-separated config keys for a DTO type.
    pub fn keys<T: for<'a> Facet<'a>>(&self) -> Vec<String> {
        list_keys(T::SHAPE, "")
    }

    /// Lists all config keys with their help text and example values.
    pub fn key_entries<T: for<'a> Facet<'a>>(&self) -> Vec<KeyEntry> {
        list_key_entries(T::SHAPE, "", &self.examples)
    }

    /// Merges non-None fields from `overlay` onto a clone of `base`.
    pub fn overlay<Base, Overlay>(
        &self,
        base: &Base,
        overlay: &Overlay,
    ) -> Result<Base, ConfigError>
    where
        Base: for<'a> Facet<'a> + Clone,
        Overlay: for<'a> Facet<'a>,
    {
        crate::reflect::overlay_dto(base, overlay)
    }

    /// Builds a typed `Reader` struct from a DTO, applying defaults and optional root resolution.
    ///
    /// `root_resolver` resolves cross-config references (e.g. a c8y mapper DTO falling back to
    /// `device.cert_path` from the root config when its own value is unset).
    /// `display_prefix` is prepended to keys in error messages (e.g. `"mappers.c8y."`)
    /// — pass `""` when no prefix is needed.
    pub fn build_reader<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
        &self,
        dto: &Dto,
        root_resolver: Option<&dyn Fn(&str) -> Option<String>>,
        display_prefix: &str,
    ) -> Result<Reader, ConfigError> {
        build_reader_at(dto, &self.defaults, root_resolver, display_prefix)
    }

    /// Applies environment variable overrides to the DTO using the configured prefix.
    ///
    /// `exclude_prefixes` is used for more specific namespaces such as
    /// `TEDGE_MAPPER_`. Returns warnings for unknown keys.
    pub fn apply_env<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        env: &EnvOverrides,
        exclude_prefixes: &[&str],
    ) -> Vec<String> {
        let Some(prefix) = &self.env_prefix else {
            return Vec::new();
        };
        let all_keys = self.keys::<T>();
        env.apply_with_prefix_excluding(dto, prefix, &all_keys, exclude_prefixes)
    }

    /// Applies mapper environment variables for a cloud such as `c8y` or `aws`.
    ///
    /// e.g. `cloud="c8y", profile=Some("staging")` looks for vars like `C8Y_STAGING_URL`.
    /// Returns warnings for unknown keys.
    pub fn apply_cloud_env<T: for<'a> Facet<'a>>(
        &self,
        dto: &mut T,
        cloud: &str,
        profile: Option<&str>,
        env: &EnvOverrides,
    ) -> Vec<String> {
        let all_keys = self.keys::<T>();
        env.apply_for_cloud(dto, cloud, profile, &all_keys)
    }

    pub(crate) fn defaults(&self) -> &DefaultsRegistry {
        &self.defaults
    }
}
