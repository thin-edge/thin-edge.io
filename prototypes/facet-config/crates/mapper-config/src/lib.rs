pub mod c8y;
pub mod shared;

use facet_config_runtime::*;
use std::fmt;

facet_config_macro::define_config! {
    Mapper {
        /// Cloud type for this mapper
        #[tedge_config(default(value = "custom"))]
        cloud_type: CloudSchema,

        /// Cloud endpoint URL
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        url: HostPort<HTTPS_PORT>,

        /// Identity of the device this mapper connects to the cloud
        device: extern shared::MapperDeviceConfig,
    }
}

#[derive(Debug)]
pub enum LoadedMapper {
    Custom(MapperConfig),
    C8y(c8y::C8yMapperConfig),
}

/// Maps a mapper's `cloud_type` value to the schema governing its `mapper.toml`
///
/// Every operation that depends on which DTO type a `cloud_type` implies is a
/// method on this enum, so the dispatch lives in exactly one place
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet, serde::Serialize, serde::Deserialize)]
#[repr(C)]
#[serde(rename_all = "lowercase")]
pub enum CloudSchema {
    C8y,
    Custom,
    Az,
    Aws,
}

impl fmt::Display for CloudSchema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::C8y => write!(f, "c8y"),
            Self::Az => write!(f, "az"),
            Self::Aws => write!(f, "aws"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl AppendRemoveItem for CloudSchema {
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

impl std::str::FromStr for CloudSchema {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "c8y" => Ok(Self::C8y),
            "custom" => Ok(Self::Custom),
            "az" => Ok(Self::Az),
            "aws" => Ok(Self::Aws),
            other => Err(format!("invalid cloud_type: {other}")),
        }
    }
}

impl CloudSchema {
    /// Resolves the schema from the `cloud_type` key in the mapper's file,
    /// falling back to the mapper's name for builtin clouds
    pub fn resolve(config_dir: &std::path::Path, name: &str) -> Self {
        Self::from_cloud_type(&resolve_cloud_type(config_dir, name))
    }

    pub fn from_cloud_type(cloud_type: &str) -> Self {
        match cloud_type {
            "c8y" => Self::C8y,
            _ => Self::Custom,
        }
    }

    fn source(
        self,
        config_dir: &std::path::Path,
        name: &str,
        env: &EnvOverrides,
    ) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError>
    {
        match self {
            Self::C8y => c8y::source(config_dir, name, builtin_profile(name), env),
            // TODO custom schemas for az and aws will be implemented in the future
            Self::Custom | Self::Az | Self::Aws => custom_source(config_dir, name, env),
        }
    }

    fn load(
        self,
        config_dir: &std::path::Path,
        name: &str,
        root: &dyn facet_config_runtime::ops::ConfigOps,
    ) -> Result<LoadedMapper, facet_config_runtime::ConfigError> {
        let path = mapper_toml_path(config_dir, name);
        let root_keys: Vec<String> = root.entries().into_iter().map(|e| e.key).collect();
        let resolve_root = |key: &str| root.read(key, None);
        match self {
            Self::C8y => {
                let mgr = c8y::config_manager(config_dir);
                let ops =
                    facet_config_runtime::ops::TypedConfigOps::<c8y::C8yMapperConfigDto>::new(
                        mgr, path,
                    )?;
                ops.manager()
                    .validate_root_dependencies(&root_keys, &format!("{name}."))?;
                let config = ops
                    .manager()
                    .build_reader::<c8y::C8yMapperConfigDto, c8y::C8yMapperConfig>(
                        ops.dto(),
                        Some(&resolve_root),
                        name,
                    )?;
                Ok(LoadedMapper::C8y(config))
            }
            // TODO custom schemas for az and aws will be implemented in the future
            Self::Custom | Self::Aws | Self::Az => {
                let mgr = config_manager(config_dir);
                let ops =
                    facet_config_runtime::ops::TypedConfigOps::<MapperConfigDto>::new(mgr, path)?;
                ops.manager()
                    .validate_root_dependencies(&root_keys, &format!("{name}."))?;
                let config = ops
                    .manager()
                    .build_reader::<MapperConfigDto, MapperConfig>(
                        ops.dto(),
                        Some(&resolve_root),
                        name,
                    )?;
                Ok(LoadedMapper::Custom(config))
            }
        }
    }

    fn reserialize(
        self,
        content: &str,
        path: &std::path::Path,
    ) -> Result<String, facet_config_runtime::ConfigError> {
        match self {
            Self::C8y => reserialize::<c8y::C8yMapperConfigDto>(content, path),
            // TODO custom schemas for az and aws will be implemented in the future
            Self::Custom | Self::Aws | Self::Az => reserialize::<MapperConfigDto>(content, path),
        }
    }
}

pub fn builtin_cloud(name: &str) -> Option<&'static str> {
    let base = name.split('.').next().unwrap_or(name);
    BUILTIN_MAPPERS.iter().find(|&&b| b == base).copied()
}

pub fn builtin_profile(name: &str) -> Option<&str> {
    name.split_once('.').map(|(_, p)| p)
}

pub fn config_manager(config_dir: &std::path::Path) -> ConfigManager {
    ConfigManager::from_schema::<MapperConfig>(config_dir)
}

/// Loads the mapper configuration as a `ConfigOps` so the CLI code can read the values dynamically
pub fn source(
    config_dir: &std::path::Path,
    name: &str,
    env: &EnvOverrides,
) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError> {
    CloudSchema::resolve(config_dir, name).source(config_dir, name, env)
}

/// Loads the mapper configuration as as a struct so the code can read the values directly
///
/// `root` is the root config the mapper's `from_root` defaults fall back to.
/// Requiring it here means a mapper config cannot be loaded without a root
/// config, and its `from_root` references are validated against the root
/// schema before any value is read
pub fn load(
    config_dir: &std::path::Path,
    name: &str,
    root: &dyn facet_config_runtime::ops::ConfigOps,
) -> Result<LoadedMapper, facet_config_runtime::ConfigError> {
    CloudSchema::resolve(config_dir, name).load(config_dir, name, root)
}

/// Rewrites a mapper's TOML file under the schema selected by its current
/// `cloud_type`, keeping the keys shared with that schema and deleting the rest
pub fn normalize_schema(
    config_dir: &std::path::Path,
    name: &str,
) -> Result<(), facet_config_runtime::ConfigError> {
    let path = mapper_toml_path(config_dir, name);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(ConfigError::IoError(format!(
                "reading {}: {e}",
                path.display()
            )))
        }
    };

    let normalized = CloudSchema::resolve(config_dir, name).reserialize(&content, &path)?;
    std::fs::write(&path, normalized)
        .map_err(|e| ConfigError::IoError(format!("{}: {e}", path.display())))
}

const BUILTIN_MAPPERS: &[&str] = &["c8y", "az", "aws"];

fn custom_source(
    config_dir: &std::path::Path,
    name: &str,
    env: &EnvOverrides,
) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError> {
    let mgr = config_manager(config_dir);
    let path = mapper_toml_path(config_dir, name);
    let mut ops = facet_config_runtime::ops::TypedConfigOps::<MapperConfigDto>::new(mgr, path)?;

    let env_name = name.replace('.', "_").to_ascii_uppercase();
    let prefix = format!("TEDGE_MAPPERS_{env_name}_");
    for warning in ops.apply_env_with_prefix(&prefix, env) {
        eprintln!("Warning: {warning}");
    }

    Ok(Box::new(ops))
}

fn mapper_toml_path(config_dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    config_dir.join("mappers").join(name).join("mapper.toml")
}

fn reserialize<T: serde::de::DeserializeOwned + serde::Serialize>(
    content: &str,
    path: &std::path::Path,
) -> Result<String, facet_config_runtime::ConfigError> {
    let dto: T = toml::from_str(content)
        .map_err(|e| ConfigError::IoError(format!("parsing {}: {e}", path.display())))?;
    toml::to_string_pretty(&dto)
        .map_err(|e| ConfigError::IoError(format!("serialization error: {e}")))
}

fn resolve_cloud_type(config_dir: &std::path::Path, name: &str) -> String {
    let path = mapper_toml_path(config_dir, name);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(table) = content.parse::<toml::Table>() {
            if let Some(toml::Value::String(ct)) = table.get("cloud_type") {
                return ct.clone();
            }
        }
    }
    builtin_cloud(name).unwrap_or("custom").to_owned()
}
