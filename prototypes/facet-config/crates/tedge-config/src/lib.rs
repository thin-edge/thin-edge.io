use facet_config_runtime::*;

facet_config_macro::define_config! {
    TEdge {
        device: {
            /// Unique device identifier
            #[tedge_config(
                example = "my-device-001",
                example = "AINA12345678",
                default(from_key_via(key = "device.cert_path", function = "certificate_common_name"))
            )]
            id: String,

            /// Device type identifier
            #[tedge_config(rename = "type", default(value = "thin-edge.io"))]
            ty: String,

            /// Path to the device certificate
            #[tedge_config(default(from_config_dir = "device-certs/tedge-certificate.pem"))]
            cert_path: camino::Utf8PathBuf,

            /// Path to the device private key
            #[tedge_config(default(from_config_dir = "device-certs/tedge-private-key.pem"))]
            key_path: camino::Utf8PathBuf,

            /// A port
            #[tedge_config(default(value = "1738"))]
            port: u16,
        },

        mqtt: {
            /// MQTT broker port
            #[tedge_config(default(value = "1883"), deprecated_key = "mqtt.external.port")]
            port: u16,

            /// MQTT broker hostname
            #[tedge_config(default(value = "localhost"))]
            host: String,

            /// MQTT client bind address
            #[tedge_config(default(value = "127.0.0.1"))]
            bind_address: std::net::IpAddr,

        },
    }
}

pub fn config_manager(config_dir: &std::path::Path) -> ConfigManager {
    ConfigManager::new(
        build_defaults(config_dir),
        build_registry(),
        build_read_only_keys(),
        build_aliases(),
        build_examples(),
    )
    .with_env_prefix("TEDGE_")
}

pub fn source(
    config_dir: &std::path::Path,
    env: &EnvOverrides,
) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError> {
    let mgr = config_manager(config_dir);
    let path = config_dir.join("tedge.toml");
    let mut ops = facet_config_runtime::ops::TypedConfigOps::<TEdgeConfigDto>::new(mgr, path)?;
    for warning in ops.apply_env_excluding(env, EXCLUDED_ENV_PREFIXES) {
        eprintln!("Warning: {warning}");
    }
    Ok(Box::new(ops))
}

pub fn load(
    config_dir: &std::path::Path,
    env: &EnvOverrides,
) -> Result<TEdgeConfig, facet_config_runtime::ConfigError> {
    let mgr = config_manager(config_dir);
    let path = config_dir.join("tedge.toml");
    let mut ops = facet_config_runtime::ops::TypedConfigOps::<TEdgeConfigDto>::new(mgr, path)?;
    for warning in ops.apply_env_excluding(env, EXCLUDED_ENV_PREFIXES) {
        eprintln!("Warning: {warning}");
    }
    ops.manager()
        .build_reader::<TEdgeConfigDto, TEdgeConfig>(ops.dto(), None, "")
}

const EXCLUDED_ENV_PREFIXES: &[&str] = &[
    "TEDGE_CONFIG_DIR",
    "TEDGE_CLOUD_PROFILE",
    "TEDGE_MAPPERS_",
    "TEDGE_C8Y_",
    "TEDGE_AZ_",
    "TEDGE_AWS_",
];
