use camino::Utf8Path;
use std::collections::HashMap;
use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ops::Action;
use tedge_config_engine::ops::ConfigOps;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_config_engine::EnvOverrides;
use tedge_config_engine::KeyEntry;
use tedge_config_engine::RootDependency;
use tedge_config_engine::RootResolver;
use tedge_utils::paths::ManagedFile;
use tedge_utils::paths::TedgePaths;

pub use mapper::MapperConfig;
pub use mapper::MapperConfigDto;
pub use root_stub::RootStubConfig;
pub use root_stub::RootStubConfigDto;

pub mod mapper {
    use camino::Utf8PathBuf;
    use tedge_config_engine::*;

    use crate::auth_method::AuthMethod;
    use crate::cloud_type::CloudType;
    use crate::seconds::SecondsOrHumanTime;
    use crate::tls_enable::TlsEnable;

    tedge_config_engine_macro::define_config! {
        Mapper {
            /// The built-in cloud integration this mapper uses: c8y, az or aws
            cloud_type: CloudType,

            /// Cloud broker URL in host:port format
            url: String,

            device: {
                /// MQTT client ID
                id: String,

                /// Path to the device certificate
                #[tedge_config(default(from_root = "device.cert_path"))]
                cert_path: Utf8PathBuf,

                /// Path to the device private key
                #[tedge_config(default(from_root = "device.key_path"))]
                key_path: Utf8PathBuf,

                /// Path to the root certificate
                #[tedge_config(default(from_root = "device.root_cert_path"))]
                root_cert_path: Utf8PathBuf,
            },

            bridge: {
                /// Use a clean MQTT session
                #[tedge_config(default(value = "false"))]
                clean_session: bool,

                /// MQTT keepalive interval
                keepalive_interval: SecondsOrHumanTime,

                tls: {
                    /// Whether to use TLS: true, false, or auto to decide from the port
                    #[tedge_config(default(value = "auto"))]
                    enable: TlsEnable,
                },

                /// Maximum MQTT payload size
                #[tedge_config(default(value = "268435455"))]
                max_payload_size: u32,
            },

            /// Authentication method
            #[tedge_config(default(value = "auto"))]
            auth_method: AuthMethod,

            /// Path to a credentials file
            credentials_path: Utf8PathBuf,
        }
    }
}

pub mod root_stub {
    use camino::Utf8PathBuf;
    use tedge_config_engine::*;

    tedge_config_engine_macro::define_config! {
        RootStub {
            device: {
                /// Path to the device certificate
                #[tedge_config(default(value = "/etc/tedge/device-certs/tedge-certificate.pem"))]
                cert_path: Utf8PathBuf,

                /// Path to the device private key
                #[tedge_config(default(value = "/etc/tedge/device-certs/tedge-private-key.pem"))]
                key_path: Utf8PathBuf,

                /// Path to the root certificate
                #[tedge_config(default(value = "/etc/ssl/certs"))]
                root_cert_path: Utf8PathBuf,
            },
        }
    }
}

/// Returns `true` for built-in mapper names (`c8y`, `az`, `aws`) and profiled
/// variants like `c8y.prod` or `aws.staging`
pub fn is_builtin_mapper_name(name: &str) -> bool {
    matches!(name, "c8y" | "az" | "aws")
        || matches!(name.split_once('.'), Some(("c8y" | "az" | "aws", profile)) if !profile.is_empty())
}

/// Generates completion entries for a built-in mapper that has no directory yet
pub fn builtin_mapper_entries(config_dir: &Utf8Path, name: &str) -> Vec<KeyEntry> {
    let mgr = ConfigManager::from_schema::<MapperConfig>(config_dir.as_std_path());
    let prefix = format!("mappers.{name}.");
    mgr.key_entries::<MapperConfigDto>()
        .into_iter()
        .map(|e| KeyEntry {
            key: format!("{prefix}{}", e.key),
            ..e
        })
        .collect()
}

pub const BUILTIN_MAPPER_NAMES: &[&str] = &["c8y", "az", "aws"];

/// Extracts the builtin mapper directory name from a full config key like
/// `mappers.c8y.url` or `mappers.c8y.profiles.prod.device.id`
pub fn extract_builtin_mapper_name(key: &str) -> Option<String> {
    let rest = key.strip_prefix("mappers.")?;
    let (cloud, rest) = rest.split_once('.')?;
    if !BUILTIN_MAPPER_NAMES.contains(&cloud) {
        return None;
    }
    match rest
        .strip_prefix("profiles.")
        .and_then(|r| r.split_once('.'))
    {
        Some((profile, _)) => Some(format!("{cloud}.{profile}")),
        None => Some(cloud.to_owned()),
    }
}

/// Turns a `tedge mapper config get` key such as `c8y.prod.url` into its `mappers.*` key
pub async fn mapper_key_from_dir_key(key: &str, config_root: &TedgePaths) -> String {
    let mut dirs = mapper_dirs(config_root).await;
    // Longest first, so `c8y.prod.url` matches `c8y.prod` rather than `c8y`
    dirs.sort_by_key(|name| std::cmp::Reverse(name.len()));
    for name in dirs {
        if let Some(rest) = key.strip_prefix(&name).and_then(|r| r.strip_prefix('.')) {
            return format!("{}{rest}", mount_prefix(&name));
        }
    }
    format!("mappers.{key}")
}

/// The key prefix a mapper directory is mounted at:
/// `c8y.prod` (a built-in cloud profile) at `mappers.c8y.profiles.prod.`,
/// any other directory at `mappers.<name>.`
fn mount_prefix(dir_name: &str) -> String {
    match dir_name.split_once('.') {
        Some((cloud, profile)) if is_builtin_mapper_name(dir_name) => {
            format!("mappers.{cloud}.profiles.{profile}.")
        }
        _ => format!("mappers.{dir_name}."),
    }
}

/// The mapper directories under `{config_dir}/mappers/`, sorted by name
async fn mapper_dirs(config_root: &TedgePaths) -> Vec<String> {
    let Ok(mappers) = config_root.dir("mappers") else {
        return Vec::new();
    };
    let mut dirs = mappers.subdirectories().await.unwrap_or_default();
    dirs.sort();
    dirs
}

/// Assembles a `FederatedConfig` from the root `tedge.toml` and all
/// mapper directories discovered under `{config_dir}/mappers/`.
pub async fn load_federated_config(
    config_root: &TedgePaths,
) -> Result<FederatedConfig, ConfigError> {
    load_federated_config_with_env(config_root, &EnvOverrides::from_env()).await
}

pub(crate) async fn load_federated_config_with_env(
    config_root: &TedgePaths,
    env: &EnvOverrides,
) -> Result<FederatedConfig, ConfigError> {
    let config_dir = config_root.root();
    let mut fed = FederatedConfig::new(config_dir.as_std_path());

    let root_mgr = ConfigManager::from_schema::<RootStubConfig>(config_dir.as_std_path());
    let root_ops = TypedConfigOps::<RootStubConfigDto>::load(
        root_mgr,
        managed_file(config_root, "tedge.toml"),
    )
    .await;
    fed.mount("", Box::new(root_ops))?;

    let mappers: Vec<_> = mapper_dirs(config_root)
        .await
        .into_iter()
        .map(|name| {
            let mount_prefix = mount_prefix(&name);
            let env_prefix = format!(
                "TEDGE_{}",
                mount_prefix.to_uppercase().replace(['.', '-'], "_")
            );
            MapperMount {
                name,
                mount_prefix,
                env_prefix,
            }
        })
        .collect();
    let mut env_prefix_owners: HashMap<&str, Vec<&str>> = HashMap::new();
    for mapper in &mappers {
        env_prefix_owners
            .entry(&mapper.env_prefix)
            .or_default()
            .push(&mapper.name);
    }

    for MapperMount {
        name,
        mount_prefix,
        env_prefix,
        ..
    } in &mappers
    {
        let mapper_mgr = ConfigManager::from_schema::<MapperConfig>(config_dir.as_std_path());
        let owners = &env_prefix_owners[env_prefix.as_str()];
        if let [first, second, ..] = owners.as_slice() {
            // Either directory could be the one the variables are meant for,
            // so neither is loaded until one of them is renamed
            let colliding = CollidingMapper {
                first: first.to_string(),
                second: second.to_string(),
                prefix: env_prefix.clone(),
                manager: mapper_mgr,
            };
            fed.mount(mount_prefix, Box::new(colliding))?;
            continue;
        }
        let mut mapper_ops =
            TypedConfigOps::<MapperConfigDto>::load(mapper_mgr, mapper_file(config_root, name))
                .await;
        mapper_ops.apply_env_with_prefix(env_prefix, env);
        fed.mount(mount_prefix, Box::new(mapper_ops))?;
    }

    Ok(fed)
}

/// Mounts the built-in mapper that `key` belongs to if it has no directory yet,
/// so a write to it creates the directory along with the file
pub fn mount_builtin_mapper(
    fed: &mut FederatedConfig,
    config_root: &TedgePaths,
    key: &str,
) -> Result<(), ConfigError> {
    let Some(name) = extract_builtin_mapper_name(key) else {
        return Ok(());
    };
    let prefix = mount_prefix(&name);
    if fed.mount_prefixes().contains(&prefix) {
        return Ok(());
    }
    let mgr = ConfigManager::from_schema::<MapperConfig>(config_root.root().as_std_path());
    let ops = TypedConfigOps::<MapperConfigDto>::empty(mgr, mapper_file(config_root, &name));
    fed.mount(&prefix, Box::new(ops))
}

fn mapper_file(config_root: &TedgePaths, name: &str) -> ManagedFile {
    managed_file(config_root, format!("mappers/{name}/mapper.toml"))
}

fn managed_file(config_root: &TedgePaths, path: impl AsRef<Utf8Path>) -> ManagedFile {
    config_root
        .file(path)
        .expect("a relative path without `..` stays inside the config root")
}

/// Where a mapper directory is mounted, and the prefix of its environment variables
struct MapperMount {
    name: String,
    mount_prefix: String,
    env_prefix: String,
}

/// A mapper whose directory name gives the same environment variable prefix as
/// another's; it fails every read and write, but still lists its keys so they
/// are routed to it
struct CollidingMapper {
    first: String,
    second: String,
    prefix: String,
    manager: ConfigManager,
}

impl CollidingMapper {
    fn error(&self) -> ConfigError {
        ConfigError::EnvPrefixCollision {
            first: self.first.clone(),
            second: self.second.clone(),
            prefix: self.prefix.clone(),
        }
    }
}

#[async_trait::async_trait]
impl ConfigOps for CollidingMapper {
    fn read(&self, _key: &str, _root: RootResolver<'_>) -> Result<Option<String>, ConfigError> {
        Err(self.error())
    }

    async fn mutate(&mut self, _key: &str, _action: Action) -> Result<(), ConfigError> {
        Err(self.error())
    }

    fn entries(&self) -> Vec<KeyEntry> {
        self.manager.key_entries::<MapperConfigDto>()
    }

    fn root_dependencies(&self) -> Vec<RootDependency> {
        // It never reads a value, so has no `from_root` default to resolve
        Vec::new()
    }
}
