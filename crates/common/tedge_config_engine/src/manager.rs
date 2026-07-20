use facet::Facet;
use facet::Shape;

use crate::append_remove::AppendRemoveRegistry;
use crate::defaults::config_get_with_defaults;
use crate::defaults::DefaultsRegistry;
use crate::defaults::EnvOverrides;
use crate::defaults::RootDependency;
use crate::reader::build_reader_at;
use crate::reflect::check_read_only;
use crate::reflect::config_add;
use crate::reflect::config_get;
use crate::reflect::config_remove;
use crate::reflect::config_set;
use crate::reflect::config_unset;
use crate::reflect::list_key_entries;
use crate::reflect::list_keys;
use crate::reflect::ConfigError;
use crate::reflect::KeyAliases;
use crate::reflect::KeyEntry;
use crate::schema::ConfigSchema;

/// Facade over the config reflection/defaults/reader subsystems.
///
/// Holds the registries (defaults, append-remove rules, aliases) and exposes
/// high-level operations on any DTO that implements `Facet`. Read-only checks
/// and example values are read directly from facet attributes on the DTO shape
/// at the point of use.
pub struct ConfigManager {
    defaults: DefaultsRegistry,
    append_remove: AppendRemoveRegistry,
    aliases: KeyAliases,
    dto_shape: &'static Shape,
    env_prefix: Option<String>,
}

impl ConfigManager {
    /// Creates a manager from a schema defined by `define_config!`.
    pub fn from_schema<S: ConfigSchema>(config_dir: &std::path::Path) -> Self {
        let mut append_remove = AppendRemoveRegistry::new();
        S::register_types(&mut append_remove);
        Self {
            defaults: DefaultsRegistry::new(S::defaults(config_dir))
                .unwrap_or_else(|e| panic!("invalid defaults registry: {e}")),
            append_remove,
            aliases: KeyAliases::from_shape(<S::Dto as Facet>::SHAPE),
            dto_shape: <S::Dto as Facet>::SHAPE,
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
        self.read_with_root(dto, key, None)
    }

    /// Returns the effective value for `key`, resolving `from_root` defaults
    /// through `root_resolver`.
    pub fn read_with_root<T: for<'a> Facet<'a>>(
        &self,
        dto: &T,
        key: &str,
        root_resolver: crate::defaults::RootResolver<'_>,
    ) -> Result<Option<String>, ConfigError> {
        config_get_with_defaults(dto, key, &self.defaults, root_resolver)
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

    /// Returns an error if `key` is marked read-only via `tedge::readonly`.
    pub fn check_read_only(&self, key: &str) -> Result<(), ConfigError> {
        check_read_only(self.dto_shape, key)
    }

    /// Returns the `from_root` references declared by this schema.
    pub fn root_dependencies(&self) -> Vec<RootDependency> {
        self.defaults.root_dependencies()
    }

    /// Rejects `from_root` references to keys the root config does not define.
    ///
    /// Call this when the root config becomes available, so a misspelled
    /// `from_root` key is reported as a schema error up front instead of
    /// reading as an unset value later.
    pub fn validate_root_dependencies(
        &self,
        root_keys: &[String],
        display_prefix: &str,
    ) -> Result<(), ConfigError> {
        crate::defaults::validate_root_dependencies(
            &self.defaults.root_dependencies(),
            root_keys,
            display_prefix,
        )
    }

    /// Resolves a potentially-aliased key to the canonical key.
    /// Returns (canonical_key, Some(deprecated_key)) if the key was aliased.
    pub fn resolve_key(&self, key: &str) -> (String, Option<&str>) {
        self.aliases.resolve(key)
    }

    /// Lists all dot-separated config keys for a DTO type.
    pub fn keys<T: for<'a> Facet<'a>>(&self) -> Vec<String> {
        list_keys(T::SHAPE, "")
    }

    /// Lists all config keys with their help text and example values.
    pub fn key_entries<T: for<'a> Facet<'a>>(&self) -> Vec<KeyEntry> {
        list_key_entries(T::SHAPE, "")
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
    /// `display_prefix` is prepended to keys in error messages (e.g. `"c8y"`)
    /// — pass `""` when no prefix is needed.
    /// `profile` is attached to [OptionalConfig](crate::OptionalConfig) values for user-facing messages.
    pub fn build_reader<Dto: for<'a> Facet<'a>, Reader: for<'a> Facet<'a>>(
        &self,
        dto: &Dto,
        root_resolver: crate::defaults::RootResolver<'_>,
        display_prefix: &str,
        profile: Option<&str>,
    ) -> Result<Reader, ConfigError> {
        build_reader_at(dto, &self.defaults, root_resolver, display_prefix, profile)
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
}
