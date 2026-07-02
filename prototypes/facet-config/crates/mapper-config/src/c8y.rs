use facet_config_runtime::*;

facet_config_macro::define_config! {
    C8yMapper {
        /// Cloud type for this mapper
        #[tedge_config(default(value = "c8y"))]
        cloud_type: String,

        /// Endpoint URL of Cumulocity tenant
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        url: HostPort<HTTPS_PORT>,

        smartrest: {
            /// SmartREST template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(value = ""))]
            templates: TemplatesSet,

            /// Use operation IDs in SmartREST messages
            #[tedge_config(default(value = "true"))]
            use_operation_id: bool,
        },

        device: {
            /// Path to the device certificate
            #[tedge_config(default(from_root = "device.cert_path"))]
            cert_path: camino::Utf8PathBuf,

            /// Path to the device private key
            #[tedge_config(default(from_root = "device.key_path"))]
            key_path: camino::Utf8PathBuf,
        },

        proxy: {
            bind: {
                /// Port the proxy listens on
                #[tedge_config(default(value = "8001"))]
                port: u16,

                /// Address the proxy binds to
                #[tedge_config(default(value = "127.0.0.1"))]
                address: std::net::IpAddr,
            },
            client: {
                /// Port the proxy client connects to
                #[tedge_config(default(from_key = "proxy.bind.port"), readonly)]
                port: u16,
            },
        },

        availability: {
            /// Enable sending heartbeat to Cumulocity
            #[tedge_config(default(value = "true"))]
            enable: bool,

            /// Heartbeat interval sent to Cumulocity as c8y_RequiredAvailability
            #[tedge_config(default(value = "3600"))]
            interval: u64,
        },

        enable: {
            /// Enable log_upload feature
            #[tedge_config(default(value = "true"))]
            log_upload: bool,

            /// Enable config_snapshot feature
            #[tedge_config(default(value = "true"))]
            config_snapshot: bool,

            /// Enable config_update feature
            #[tedge_config(default(value = "true"))]
            config_update: bool,
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
}

pub fn source(
    config_dir: &std::path::Path,
    name: &str,
    profile: Option<&str>,
    env: &EnvOverrides,
) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError> {
    let mgr = config_manager(config_dir);
    let path = config_dir.join("mappers").join(name).join("mapper.toml");
    let mut ops = facet_config_runtime::ops::TypedConfigOps::<C8yMapperConfigDto>::new(mgr, path)?;

    let env_name = name.replace('.', "_").to_ascii_uppercase();
    let prefix = format!("TEDGE_MAPPERS_{env_name}_");
    for warning in ops.apply_env_with_prefix(&prefix, env) {
        eprintln!("Warning: {warning}");
    }

    for warning in ops.apply_cloud_env("c8y", profile, env) {
        eprintln!("Warning: {warning}");
    }

    Ok(Box::new(ops))
}
