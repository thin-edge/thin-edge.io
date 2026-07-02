pub mod c8y;

use facet_config_runtime::*;

facet_config_macro::define_config! {
    Mapper {
        /// Cloud type for this mapper
        #[tedge_config(default(value = "custom"))]
        cloud_type: String,

        /// Cloud endpoint URL
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        url: HostPort<HTTPS_PORT>,

        device: {
            /// Path to the device certificate for this mapper
            #[tedge_config(default(from_root = "device.cert_path"))]
            cert_path: camino::Utf8PathBuf,

            /// Path to the device private key for this mapper
            #[tedge_config(default(from_root = "device.key_path"))]
            key_path: camino::Utf8PathBuf,
        },
    }
}

#[derive(Debug)]
pub enum LoadedMapper {
    Custom(MapperConfig),
    C8y(c8y::C8yMapperConfig),
}

pub fn builtin_cloud(name: &str) -> Option<&'static str> {
    let base = name.split('.').next().unwrap_or(name);
    BUILTIN_MAPPERS.iter().find(|&&b| b == base).copied()
}

pub fn builtin_profile(name: &str) -> Option<&str> {
    name.split_once('.').map(|(_, p)| p)
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

/// Loads the mapper configuration as a `ConfigOps` so the CLI code can read the values dynamically
pub fn source(
    config_dir: &std::path::Path,
    name: &str,
    env: &EnvOverrides,
) -> Result<Box<dyn facet_config_runtime::ops::ConfigOps>, facet_config_runtime::ConfigError> {
    let cloud_type = resolve_cloud_type(config_dir, name);
    let profile = builtin_profile(name);

    if cloud_type == "c8y" {
        return c8y::source(config_dir, name, profile, env);
    }

    let mgr = config_manager(config_dir);
    let path = config_dir.join("mappers").join(name).join("mapper.toml");
    let mut ops = facet_config_runtime::ops::TypedConfigOps::<MapperConfigDto>::new(mgr, path)?;

    let env_name = name.replace('.', "_").to_ascii_uppercase();
    let prefix = format!("TEDGE_MAPPERS_{env_name}_");
    for warning in ops.apply_env_with_prefix(&prefix, env) {
        eprintln!("Warning: {warning}");
    }

    Ok(Box::new(ops))
}

/// Loads the mapper configuration as as a struct so the code can read the values directly
pub fn load(
    config_dir: &std::path::Path,
    name: &str,
) -> Result<LoadedMapper, facet_config_runtime::ConfigError> {
    let cloud_type = resolve_cloud_type(config_dir, name);
    let path = config_dir.join("mappers").join(name).join("mapper.toml");

    if cloud_type == "c8y" {
        let mgr = c8y::config_manager(config_dir);
        let ops =
            facet_config_runtime::ops::TypedConfigOps::<c8y::C8yMapperConfigDto>::new(mgr, path)?;
        let config = ops
            .manager()
            .build_reader::<c8y::C8yMapperConfigDto, c8y::C8yMapperConfig>(ops.dto(), None, name)?;
        return Ok(LoadedMapper::C8y(config));
    }

    let mgr = config_manager(config_dir);
    let ops = facet_config_runtime::ops::TypedConfigOps::<MapperConfigDto>::new(mgr, path)?;
    let config = ops
        .manager()
        .build_reader::<MapperConfigDto, MapperConfig>(ops.dto(), None, name)?;
    Ok(LoadedMapper::Custom(config))
}

const BUILTIN_MAPPERS: &[&str] = &["c8y", "az", "aws"];

fn resolve_cloud_type(config_dir: &std::path::Path, name: &str) -> String {
    let path = config_dir.join("mappers").join(name).join("mapper.toml");
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(table) = content.parse::<toml::Table>() {
            if let Some(toml::Value::String(ct)) = table.get("cloud_type") {
                return ct.clone();
            }
        }
    }
    builtin_cloud(name).unwrap_or("custom").to_owned()
}
