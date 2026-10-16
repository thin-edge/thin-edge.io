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
#[derive(Clone)]
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
    /// Why the file could not be loaded, returned by reads so one broken file
    /// doesn't stop other sources mounted alongside it from loading
    load_error: Option<ConfigError>,
}

impl<T> TypedConfigOps<T>
where
    T: for<'a> Facet<'a> + Default,
{
    /// Loads a DTO from `path`, or starts from `T::default()` if it does not exist.
    ///
    /// A file that cannot be loaded is not an error here: reads from this
    /// source return the load error instead, and writes reload the file.
    pub fn new(manager: ConfigManager, path: PathBuf) -> Self {
        let (dto, load_error) = match load_dto(&path) {
            Ok(dto) => (dto, None),
            Err(e) => (T::default(), Some(e)),
        };
        Self {
            manager,
            dto,
            path,
            load_error,
        }
    }

    fn check_loaded(&self) -> Result<(), ConfigError> {
        self.load_error.clone().map_or(Ok(()), Err)
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
        let content = facet_toml::to_string(dto)
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
    T: for<'a> Facet<'a> + Default,
{
    fn get(&self, key: &str) -> Result<Option<String>, ConfigError> {
        self.check_loaded()?;
        self.manager.get(&self.dto, key)
    }

    fn read(&self, key: &str, root: RootResolver<'_>) -> Result<Option<String>, ConfigError> {
        self.check_loaded()?;
        self.manager.read_with_root(&self.dto, key, root)
    }

    fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError> {
        self.manager.check_read_only(key)?;
        // The DTO only holds schema keys, so saving it would delete any other key in the file
        check_no_unknown_keys(&self.path, &self.manager.keys::<T>())?;
        let mut dto = load_dto(&self.path)?;
        match action {
            Action::Set(ref value) => self.manager.set(&mut dto, key, value)?,
            Action::Unset => self.manager.unset(&mut dto, key)?,
            Action::Add(ref value) => self.manager.add(&mut dto, key, value)?,
            Action::Remove(ref value) => self.manager.remove(&mut dto, key, value)?,
        }
        self.save(&dto)?;
        self.dto = dto;
        self.load_error = None;
        Ok(())
    }

    fn entries(&self) -> Vec<KeyEntry> {
        self.manager.key_entries::<T>()
    }

    fn root_dependencies(&self) -> Vec<RootDependency> {
        self.manager.root_dependencies()
    }
}

fn load_dto<T: for<'a> Facet<'a> + Default>(path: &Path) -> Result<T, ConfigError> {
    let invalid = |reason: String| ConfigError::InvalidFile {
        path: path.to_owned(),
        reason,
    };
    match std::fs::read_to_string(path) {
        Ok(content) => facet_toml::from_str(&content).map_err(|e| invalid(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(invalid(e.to_string())),
    }
}

fn check_no_unknown_keys(path: &Path, known_keys: &[String]) -> Result<(), ConfigError> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(ConfigError::IoError(format!(
                "reading {}: {e}",
                path.display()
            )))
        }
    };
    let table: toml::Table = content
        .parse()
        .map_err(|e| ConfigError::IoError(format!("parsing {}: {e}", path.display())))?;
    let mut unknown = Vec::new();
    collect_unknown_keys(&table, "", known_keys, &mut unknown);
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::UnknownKeysInFile {
            path: path.to_owned(),
            keys: unknown,
        })
    }
}

fn collect_unknown_keys(
    table: &toml::Table,
    prefix: &str,
    known_keys: &[String],
    unknown: &mut Vec<String>,
) {
    for (name, value) in table {
        let key = format!("{prefix}{name}");
        if known_keys.contains(&key) {
            continue;
        }
        match value {
            toml::Value::Table(group) => {
                collect_unknown_keys(group, &format!("{key}."), known_keys, unknown)
            }
            _ => unknown.push(key),
        }
    }
}
