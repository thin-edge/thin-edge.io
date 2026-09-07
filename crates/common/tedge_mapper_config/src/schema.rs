use std::path::Path;
use tedge_config_engine::federated::FederatedConfig;
use tedge_config_engine::ops::TypedConfigOps;
use tedge_config_engine::ConfigError;
use tedge_config_engine::ConfigManager;
use tedge_config_engine::EnvOverrides;

pub use mapper::MapperConfig;
pub use mapper::MapperConfigDto;
pub use root_stub::RootStubConfig;
pub use root_stub::RootStubConfigDto;

pub mod mapper {
    use camino::Utf8PathBuf;
    use tedge_config_engine::*;

    use crate::auth_method::AuthMethod;

    tedge_config_engine_macro::define_config! {
        Mapper {
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
                keepalive_interval: String,

                /// TLS transport control
                #[tedge_config(default(value = "auto"))]
                tls: String,

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

/// Assembles a `FederatedConfig` from the root `tedge.toml` and all
/// mapper directories discovered under `{config_dir}/mappers/`.
pub fn load_federated_config(config_dir: impl AsRef<Path>) -> Result<FederatedConfig, ConfigError> {
    let config_dir = config_dir.as_ref();
    let env = EnvOverrides::from_env();

    let mut fed = FederatedConfig::new(config_dir);

    let root_mgr = ConfigManager::from_schema::<RootStubConfig>(config_dir);
    let mut root_ops =
        TypedConfigOps::<RootStubConfigDto>::new(root_mgr, config_dir.join("tedge.toml"))?;
    root_ops.apply_env_excluding(&env, &["mappers."]);
    fed.mount("", Box::new(root_ops))?;

    let mappers_dir = config_dir.join("mappers");
    if let Ok(entries) = std::fs::read_dir(&mappers_dir) {
        let mut mapper_names: Vec<(String, std::path::PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    mapper_names.push((name.to_owned(), entry.path()));
                }
            }
        }
        mapper_names.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (name, dir) in mapper_names {
            let toml_path = dir.join("mapper.toml");
            let mapper_mgr = ConfigManager::from_schema::<MapperConfig>(config_dir);
            let mut mapper_ops = TypedConfigOps::<MapperConfigDto>::new(mapper_mgr, toml_path)?;

            let env_prefix = format!(
                "TEDGE_CONFIG_MAPPERS_{}_",
                name.to_uppercase().replace('-', "_")
            );
            mapper_ops.apply_env_with_prefix(&env_prefix, &env);

            let prefix = format!("mappers.{name}.");
            fed.mount(&prefix, Box::new(mapper_ops))?;
        }
    }

    Ok(fed)
}
