use camino::Utf8Path;

use facet::Facet;
use tedge_utils::paths::ManagedFile;

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
#[async_trait::async_trait]
pub trait ConfigOps: Send + Sync {
    fn read(&self, key: &str, root: RootResolver<'_>) -> Result<Option<String>, ConfigError>;
    /// Reloads the persisted configuration, applies `action`, and saves it.
    ///
    /// Any environment overrides previously applied to this source are
    /// deliberately discarded before the mutation so they cannot affect the
    /// value being changed or leak into the backing file.
    async fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError>;
    fn entries(&self) -> Vec<KeyEntry>;
    /// The `from_root` references this source's schema declares.
    fn root_dependencies(&self) -> Vec<RootDependency>;
}

/// File-backed config operations for a single concrete DTO type.
pub struct TypedConfigOps<T> {
    manager: ConfigManager,
    dto: T,
    file: ManagedFile,
    /// Why the file could not be loaded, returned by reads so one broken file
    /// doesn't stop other sources mounted alongside it from loading
    load_error: Option<ConfigError>,
}

impl<T> TypedConfigOps<T>
where
    T: for<'a> Facet<'a> + Default,
{
    /// Loads a DTO from `file`, or starts from `T::default()` if it does not exist.
    ///
    /// A file that cannot be loaded is not an error here: reads from this
    /// source return the load error instead, and writes reload the file.
    pub async fn load(manager: ConfigManager, file: ManagedFile) -> Self {
        let (dto, load_error) = match load_dto(&file, &manager, None).await {
            Ok(dto) => (dto, None),
            Err(e) => (T::default(), Some(e)),
        };
        Self {
            manager,
            dto,
            file,
            load_error,
        }
    }

    /// Starts from `T::default()` without reading `file`, for a file that doesn't exist yet
    pub fn empty(manager: ConfigManager, file: ManagedFile) -> Self {
        Self {
            manager,
            dto: T::default(),
            file,
            load_error: None,
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

    async fn save(&self, dto: &T) -> Result<(), ConfigError> {
        let write_error = |reason: String| ConfigError::WriteFile {
            path: self.file.path().to_owned(),
            reason,
        };
        let content = facet_toml::to_string(dto).map_err(|e| write_error(e.to_string()))?;
        // facet_toml writes nested groups as inline tables; the toml crate writes
        // `[group]` sections, matching the docs and files written by tedge_config
        let table: toml::Table = content
            .parse()
            .map_err(|e: toml::de::Error| write_error(e.to_string()))?;
        let content = toml::to_string(&table).map_err(|e| write_error(e.to_string()))?;
        // Like tedge_config, a file that can't be given tedge ownership is still written
        let file = self.file.clone().warn_and_ignore_permission_errors();
        let parent = file.parent();
        parent
            .ensure()
            .await
            .map_err(|e| write_error(format!("creating {parent}: {e}")))?;
        file.replace_atomic(content)
            .await
            .map_err(|e| write_error(e.to_string()))
    }
}

#[async_trait::async_trait]
impl<T> ConfigOps for TypedConfigOps<T>
where
    T: for<'a> Facet<'a> + Default + Send + Sync,
{
    fn read(&self, key: &str, root: RootResolver<'_>) -> Result<Option<String>, ConfigError> {
        self.check_loaded()?;
        self.manager.read_with_root(&self.dto, key, root)
    }

    async fn mutate(&mut self, key: &str, action: Action) -> Result<(), ConfigError> {
        self.manager.check_read_only(key)?;
        // The DTO only holds schema keys, so saving it would delete any other key in the file
        let known_keys = self.manager.keys::<T>();
        let mut dto = load_dto(&self.file, &self.manager, Some(&known_keys)).await?;
        match action {
            Action::Set(ref value) => self.manager.set(&mut dto, key, value)?,
            Action::Unset => self.manager.unset(&mut dto, key)?,
            Action::Add(ref value) => self.manager.add(&mut dto, key, value)?,
            Action::Remove(ref value) => self.manager.remove(&mut dto, key, value)?,
        }
        self.save(&dto).await?;
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

/// Loads the DTO from `file`, first refusing a file with keys outside `known_keys` if given
async fn load_dto<T: for<'a> Facet<'a> + Default>(
    file: &ManagedFile,
    manager: &ConfigManager,
    known_keys: Option<&[String]>,
) -> Result<T, ConfigError> {
    let path = file.path();
    let Some(content) = file
        .read_to_string()
        .await
        .map_err(|e| invalid_file(path, e.to_string()))?
    else {
        return Ok(T::default());
    };
    let table = parse_file(path, &content)?;
    if let Some(known_keys) = known_keys {
        check_no_unknown_keys(path, &table, known_keys)?;
    }
    parse_dto(path, manager, &content, &table)
}

/// Parses the file as TOML, which facet_toml alone does not check strictly
/// (it reads `url = ` as an empty string)
fn parse_file(path: &Utf8Path, content: &str) -> Result<toml::Table, ConfigError> {
    content
        .parse()
        .map_err(|e: toml::de::Error| invalid_file(path, e.to_string()))
}

fn parse_dto<T: for<'a> Facet<'a> + Default>(
    path: &Utf8Path,
    manager: &ConfigManager,
    content: &str,
    table: &toml::Table,
) -> Result<T, ConfigError> {
    facet_toml::from_str(content).map_err(|e| {
        invalid_file(
            path,
            first_invalid_value::<T>(manager, table).unwrap_or_else(|| e.to_string()),
        )
    })
}

fn invalid_file(path: &Utf8Path, reason: String) -> ConfigError {
    ConfigError::InvalidFile {
        path: path.to_owned(),
        reason,
    }
}

/// Finds the first key whose value doesn't parse, as facet_toml's error names
/// neither the full key nor the values it accepts
fn first_invalid_value<T: for<'a> Facet<'a> + Default>(
    manager: &ConfigManager,
    table: &toml::Table,
) -> Option<String> {
    manager.keys::<T>().into_iter().find_map(|key| {
        let value = match toml_value_at(table, &key)? {
            toml::Value::String(s) => s.clone(),
            toml::Value::Array(_) | toml::Value::Table(_) => return None,
            other => other.to_string(),
        };
        let reason = match manager.set(&mut T::default(), &key, &value).err()? {
            ConfigError::ParseError(reason) => reason,
            e => e.to_string(),
        };
        Some(format!("{key}: {reason}"))
    })
}

fn toml_value_at<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut segments = key.split('.');
    let mut value = table.get(segments.next()?)?;
    for segment in segments {
        value = value.as_table()?.get(segment)?;
    }
    Some(value)
}

fn check_no_unknown_keys(
    path: &Utf8Path,
    table: &toml::Table,
    known_keys: &[String],
) -> Result<(), ConfigError> {
    let mut unknown = Vec::new();
    collect_unknown_keys(table, "", known_keys, &mut unknown);
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
