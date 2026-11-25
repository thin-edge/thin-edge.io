//! Chooses the schema that governs a mapper's `mapper.toml`

use camino::Utf8Path;
use facet::Facet;
use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigManager;
use tedge_config_engine::ConfigSchema;
use tedge_config_engine::EnvOverrides;
use tedge_config_engine::KeyEntry;
use tedge_utils::paths::ManagedFile;

use crate::schema::MapperConfig;
use crate::CloudType;

/// The schema a mapper's `cloud_type` selects
///
/// Every operation that depends on the DTO type a mapper is read with is a
/// method here, so the choice of schema is made in one place
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudSchema {
    Generic,
    C8y,
    Az,
    Aws,
}

impl CloudSchema {
    /// Returns the schema for the mapper directory `name` whose file is `file`
    ///
    /// The file's `cloud_type` decides; a file without one falls back to the
    /// directory name, so `mappers/c8y.prod/` is a c8y mapper. A file that
    /// can't be read falls back the same way, and loading it reports why.
    pub async fn resolve(name: &str, file: &ManagedFile) -> Self {
        match cloud_type_in_file(file).await {
            Some(cloud_type) => Self::from(cloud_type),
            None => Self::from_name(name),
        }
    }

    /// Returns the schema a mapper directory without a `cloud_type` uses
    pub fn from_name(name: &str) -> Self {
        let base = name.split_once('.').map_or(name, |(base, _)| base);
        match base.parse::<CloudType>() {
            Ok(cloud_type) if crate::is_builtin_mapper_name(name) => Self::from(cloud_type),
            _ => Self::Generic,
        }
    }

    /// Loads `file` under this schema, applying the environment variables
    /// that start with `env_prefix`
    pub async fn load(
        self,
        config_dir: &Utf8Path,
        file: ManagedFile,
        env_prefix: &str,
        env: &EnvOverrides,
    ) -> Box<dyn ConfigOps> {
        match self {
            // The cloud schemas land one at a time; until then every cloud
            // reads its file with the generic schema
            Self::Generic | Self::C8y | Self::Az | Self::Aws => {
                load::<MapperConfig>(config_dir, file, env_prefix, env).await
            }
        }
    }

    /// Starts an empty source under this schema, for a file not written yet
    pub fn empty(self, config_dir: &Utf8Path, file: ManagedFile) -> Box<dyn ConfigOps> {
        match self {
            Self::Generic | Self::C8y | Self::Az | Self::Aws => Box::new(TypedConfigOps::<
                <MapperConfig as ConfigSchema>::Dto,
            >::empty(
                manager::<MapperConfig>(config_dir),
                file,
            )),
        }
    }

    /// Lists the keys of this schema with their docs and examples
    pub fn entries(self, config_dir: &Utf8Path) -> Vec<KeyEntry> {
        match self {
            Self::Generic | Self::C8y | Self::Az | Self::Aws => entries::<MapperConfig>(config_dir),
        }
    }
}

impl From<CloudType> for CloudSchema {
    fn from(cloud_type: CloudType) -> Self {
        match cloud_type {
            CloudType::C8y => Self::C8y,
            CloudType::Az => Self::Az,
            CloudType::Aws => Self::Aws,
        }
    }
}

async fn cloud_type_in_file(file: &ManagedFile) -> Option<CloudType> {
    let content = file.read_to_string().await.ok()??;
    let table: toml::Table = content.parse().ok()?;
    table.get("cloud_type")?.as_str()?.parse().ok()
}

async fn load<S>(
    config_dir: &Utf8Path,
    file: ManagedFile,
    env_prefix: &str,
    env: &EnvOverrides,
) -> Box<dyn ConfigOps>
where
    S: ConfigSchema,
    S::Dto: for<'a> Facet<'a> + Default + Send + Sync + 'static,
{
    let mut ops = TypedConfigOps::<S::Dto>::load(manager::<S>(config_dir), file).await;
    ops.apply_env_with_prefix(env_prefix, env);
    Box::new(ops)
}

fn entries<S: ConfigSchema>(config_dir: &Utf8Path) -> Vec<KeyEntry> {
    manager::<S>(config_dir).key_entries::<S::Dto>()
}

fn manager<S: ConfigSchema>(config_dir: &Utf8Path) -> ConfigManager {
    ConfigManager::from_schema::<S>(config_dir.as_std_path())
}
