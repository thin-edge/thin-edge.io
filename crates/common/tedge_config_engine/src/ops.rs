use std::path::Path;
use std::path::PathBuf;

use facet::Facet;

use crate::defaults::EnvOverrides;
use crate::defaults::RootDependency;
use crate::defaults::RootResolver;
use crate::manager::ConfigManager;
use crate::reflect::ConfigError;
use crate::reflect::KeyEntry;

/// A write operation routed through the generic config backend.
pub enum Action {
    Set(String),
    Unset,
    Add(String),
    Remove(String),
}

/// Object-safe boundary used to mount different DTO types in one config space.
pub trait ConfigOps {
    fn get(&self, key: &str) -> Result<Option<String>, ConfigError>;
    fn read(&self, key: &str, root: RootResolver<'_>) -> Result<Option<String>, ConfigError>;
    /// Reloads the persisted configuration, applies `action`, and saves it.
    ///
    /// Any environment overrides previously applied to this source are
    /// deliberately discarded before the mutation so they cannot affect the
    /// value being changed or leak into the backing file.
    fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError>;
    fn entries(&self) -> Vec<KeyEntry>;
    /// The `from_root` references this source's schema declares.
    fn root_dependencies(&self) -> Vec<RootDependency>;
}

/// File-backed config operations for a single concrete DTO type.
pub struct TypedConfigOps<T> {
    manager: ConfigManager,
    dto: T,
    path: PathBuf,
}

impl<T> TypedConfigOps<T>
where
    T: for<'a> Facet<'a> + Default + serde::de::DeserializeOwned + serde::Serialize,
{
    /// Loads a DTO from `path`, or starts from `T::default()` if it does not exist.
    pub fn new(manager: ConfigManager, path: PathBuf) -> Result<Self, ConfigError> {
        let dto = load_dto(&path)?;
        Ok(Self { manager, dto, path })
    }

    /// Applies the manager's configured environment prefix to this DTO.
    pub fn apply_env(&mut self, env: &EnvOverrides) -> Vec<String> {
        self.manager.apply_env(&mut self.dto, env, &[])
    }

    /// Applies environment overrides while leaving more specific prefixes untouched.
    pub fn apply_env_excluding(
        &mut self,
        env: &EnvOverrides,
        exclude_prefixes: &[&str],
    ) -> Vec<String> {
        self.manager.apply_env(&mut self.dto, env, exclude_prefixes)
    }

    /// Applies environment overrides using a caller-provided prefix.
    pub fn apply_env_with_prefix(&mut self, prefix: &str, env: &EnvOverrides) -> Vec<String> {
        let all_keys = self.manager.keys::<T>();
        env.apply_with_prefix(&mut self.dto, prefix, &all_keys)
    }

    /// Returns the in-memory DTO after file and environment inputs have been applied.
    pub fn dto(&self) -> &T {
        &self.dto
    }

    pub fn dto_mut(&mut self) -> &mut T {
        &mut self.dto
    }

    /// Returns the manager that owns this DTO's schema registries.
    pub fn manager(&self) -> &ConfigManager {
        &self.manager
    }

    fn save(&self, dto: &T) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(dto)
            .map_err(|e| ConfigError::IoError(format!("serialization error: {e}")))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::IoError(format!("{}: {e}", parent.display())))?;
        }
        std::fs::write(&self.path, content)
            .map_err(|e| ConfigError::IoError(format!("{}: {e}", self.path.display())))?;
        Ok(())
    }
}

impl<T> ConfigOps for TypedConfigOps<T>
where
    T: for<'a> Facet<'a> + Default + serde::de::DeserializeOwned + serde::Serialize,
{
    fn get(&self, key: &str) -> Result<Option<String>, ConfigError> {
        self.manager.get(&self.dto, key)
    }

    fn read(&self, key: &str, root: RootResolver<'_>) -> Result<Option<String>, ConfigError> {
        self.manager.read_with_root(&self.dto, key, root)
    }

    fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError> {
        self.manager.check_read_only(key)?;
        let mut dto = load_dto(&self.path)?;
        match action {
            Action::Set(ref value) => self.manager.set(&mut dto, key, value)?,
            Action::Unset => self.manager.unset(&mut dto, key)?,
            Action::Add(ref value) => self.manager.add(&mut dto, key, value)?,
            Action::Remove(ref value) => self.manager.remove(&mut dto, key, value)?,
        }
        self.save(&dto)?;
        self.dto = dto;
        Ok(())
    }

    fn entries(&self) -> Vec<KeyEntry> {
        self.manager.key_entries::<T>()
    }

    fn root_dependencies(&self) -> Vec<RootDependency> {
        self.manager.root_dependencies()
    }
}

fn load_dto<T: for<'a> Facet<'a> + Default + serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<T, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content)
            .map_err(|e| ConfigError::IoError(format!("parsing {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(ConfigError::IoError(format!(
            "reading {}: {e}",
            path.display()
        ))),
    }
}
