use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use facet_config_runtime::federated::FederatedConfig;
use facet_config_runtime::ops::Action;
use facet_config_runtime::EnvOverrides;

const CLOUD_ALIASES: &[&str] = &["c8y", "az", "aws"];

fn resolve_deprecated_alias(key: &str) -> Option<String> {
    for prefix in CLOUD_ALIASES {
        if let Some(rest) = key.strip_prefix(prefix) {
            if rest.starts_with('.') || rest.is_empty() {
                return Some(format!("mappers.{prefix}{rest}"));
            }
        }
    }
    None
}

fn resolve_full_key(key: &str, profile: Option<&str>) -> String {
    let key = resolve_deprecated_alias(key).unwrap_or_else(|| key.to_owned());

    if let Some(rest) = key.strip_prefix("mappers.") {
        if let Some(dot_pos) = rest.find('.') {
            let base_name = &rest[..dot_pos];
            let mapper_key = &rest[dot_pos + 1..];

            let is_cloud = CLOUD_ALIASES.contains(&base_name);

            let name = match profile {
                Some(p) if !p.is_empty() && is_cloud => format!("{base_name}.{p}"),
                _ => base_name.to_owned(),
            };

            return format!("mappers.{name}.{mapper_key}");
        }
    }
    key
}

#[derive(Parser)]
#[command(name = "tedge-config", about = "Manage thin-edge configuration")]
struct Cli {
    #[arg(long, env = "TEDGE_CONFIG_DIR", default_value = "/etc/tedge")]
    config_dir: PathBuf,

    #[arg(long, env = "TEDGE_CLOUD_PROFILE")]
    profile: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read an effective value (TOML, defaults, and environment overrides)
    Get { key: String },
    /// Set a persistent TOML value (environment overrides are ignored)
    Set { key: String, value: String },
    /// Remove a value from TOML (environment overrides are ignored)
    Unset { key: String },
    /// Append to a persisted TOML list (environment overrides are ignored)
    Add { key: String, value: String },
    /// Remove from a persisted TOML list (environment overrides are ignored)
    Remove { key: String, value: String },
    /// List effective values (TOML, defaults, and environment overrides)
    List,
    /// Show the effective composed configuration (root + all mappers)
    Show,
    /// Output available keys for shell tab completion
    Completions,
}

fn discover_mapper_names(config_dir: &Path) -> Vec<String> {
    let mappers_dir = config_dir.join("mappers");
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&mappers_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    names.push(name.to_owned());
                }
            }
        }
    }
    names.sort();
    names
}

fn build_federated(
    config_dir: &Path,
    profile: Option<&str>,
    env: &EnvOverrides,
) -> anyhow::Result<FederatedConfig> {
    let mut fed = FederatedConfig::new(config_dir);

    fed.mount(
        "",
        tedge_config::source(config_dir, env).context("loading root config")?,
    )
    .context("mounting root config")?;

    let mut mounted = std::collections::HashSet::new();

    for &cloud in CLOUD_ALIASES {
        fed.mount(
            &format!("mappers.{cloud}."),
            mapper_config::source(config_dir, cloud, env)
                .with_context(|| format!("loading mapper '{cloud}'"))?,
        )
        .with_context(|| format!("mounting mapper '{cloud}'"))?;
        mounted.insert(cloud.to_owned());
    }

    for name in discover_mapper_names(config_dir) {
        if !mounted.contains(&name) {
            fed.mount(
                &format!("mappers.{name}."),
                mapper_config::source(config_dir, &name, env)
                    .with_context(|| format!("loading mapper '{name}'"))?,
            )
            .with_context(|| format!("mounting mapper '{name}'"))?;
            mounted.insert(name);
        }
    }

    if let Some(profile) = profile {
        if !profile.is_empty() {
            for &cloud in CLOUD_ALIASES {
                let profiled_name = format!("{cloud}.{profile}");
                if !mounted.contains(&profiled_name) {
                    fed.mount(
                        &format!("mappers.{profiled_name}."),
                        mapper_config::source(config_dir, &profiled_name, env)
                            .with_context(|| format!("loading mapper '{profiled_name}'"))?,
                    )
                    .with_context(|| format!("mounting mapper '{profiled_name}'"))?;
                }
            }
        }
    }

    Ok(fed)
}

/// Rewrites a mapper's file under its new schema after a `cloud_type` change
fn normalize_mapper_schema(config_dir: &Path, full_key: &str) -> anyhow::Result<()> {
    if let Some(name) = full_key
        .strip_prefix("mappers.")
        .and_then(|k| k.strip_suffix(".cloud_type"))
    {
        mapper_config::normalize_schema(config_dir, name)
            .with_context(|| format!("rewriting mapper '{name}' under its new schema"))?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let env = EnvOverrides::from_env();
    let profile = cli.profile.as_deref();

    let mut fed = build_federated(&cli.config_dir, profile, &env)?;

    match cli.command {
        Command::Get { key } => {
            let full_key = resolve_full_key(&key, profile);
            match fed.read(&full_key)? {
                Some(v) => println!("{v}"),
                None => {
                    let profile_hint = profile
                        .filter(|p| !p.is_empty())
                        .map(|p| format!(" (profile '{p}')"))
                        .unwrap_or_default();
                    anyhow::bail!("The value for '{key}'{profile_hint} is not set.");
                }
            }
        }
        Command::Set { key, value } => {
            let key = resolve_full_key(&key, profile);
            fed.mutate(&key, Action::Set(value))?;
            normalize_mapper_schema(&cli.config_dir, &key)?;
        }
        Command::Unset { key } => {
            let key = resolve_full_key(&key, profile);
            fed.mutate(&key, Action::Unset)?;
            normalize_mapper_schema(&cli.config_dir, &key)?;
        }
        Command::Add { key, value } => {
            let key = resolve_full_key(&key, profile);
            fed.mutate(&key, Action::Add(value))?;
        }
        Command::Remove { key, value } => {
            let key = resolve_full_key(&key, profile);
            fed.mutate(&key, Action::Remove(value))?;
        }
        Command::List => {
            for entry in fed.all_entries() {
                let value = match fed.read(&entry.key) {
                    Ok(Some(v)) => v,
                    Ok(None) => String::new(),
                    Err(_) => "<error>".into(),
                };
                let doc = entry.doc.first().map(|d| d.trim()).unwrap_or("");
                if doc.is_empty() {
                    println!("{}={value}", entry.key);
                } else {
                    println!("{}={value}\t# {doc}", entry.key);
                }
                if !entry.examples.is_empty() {
                    let label = if entry.examples.len() == 1 {
                        "Example"
                    } else {
                        "Examples"
                    };
                    println!("  {label}: {}", entry.examples.join(", "));
                }
            }
        }
        Command::Show => {
            for entry in fed.all_entries() {
                let value = match fed.read(&entry.key) {
                    Ok(Some(v)) => v,
                    Ok(None) => "<not set>".into(),
                    Err(_) => "<error>".into(),
                };
                println!("{}: {value}", entry.key);
            }
        }
        Command::Completions => {
            for entry in fed.all_entries() {
                let doc = entry.doc.first().map(|d| d.trim()).unwrap_or("");
                if doc.is_empty() {
                    println!("{}", entry.key);
                } else {
                    println!("{}\t{doc}", entry.key);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_config_runtime::{DefaultSpec, DefaultsRegistry, FieldDefault};
    use std::path::Path;

    mod root_config {
        use super::*;

        #[test]
        fn set_and_get_simple_string() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "device.id", "test-device-001").unwrap();
            assert_eq!(
                mgr.get(&dto, "device.id").unwrap(),
                Some("test-device-001".into())
            );
        }

        #[test]
        fn set_parses_u16_from_string() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "mqtt.port", "1883").unwrap();
            assert_eq!(dto.mqtt.as_ref().unwrap().port, Some(1883));
        }

        #[test]
        fn get_unset_field_returns_none() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            assert_eq!(mgr.get(&dto, "mqtt.port").unwrap(), None);
        }

        #[test]
        fn unset_clears_previously_set_value() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "mqtt.port", "1883").unwrap();
            mgr.unset(&mut dto, "mqtt.port").unwrap();
            assert_eq!(dto.mqtt.as_ref().unwrap().port, None);
        }

        #[test]
        fn unknown_key_returns_error() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            assert!(mgr.get(&dto, "nonexistent.key").is_err());
        }

        #[test]
        fn invalid_parse_returns_error() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            assert!(mgr.set(&mut dto, "mqtt.port", "not-a-number").is_err());
        }

        #[test]
        fn renamed_field_uses_facet_name() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "device.type", "thin-edge.io").unwrap();
            assert_eq!(dto.device.as_ref().unwrap().ty, Some("thin-edge.io".into()));
            assert_eq!(
                mgr.get(&dto, "device.type").unwrap(),
                Some("thin-edge.io".into())
            );
        }

        #[test]
        fn list_keys_enumerates_all_leaf_fields() {
            let mgr = root_mgr();
            let mut keys = mgr.keys::<tedge_config::TEdgeConfigDto>();
            keys.sort();
            pretty_assertions::assert_eq!(
                keys,
                vec![
                    "device.cert_path",
                    "device.id",
                    "device.key_path",
                    "device.port",
                    "device.type",
                    "mqtt.bind_address",
                    "mqtt.host",
                    "mqtt.port",
                ]
            );
        }

        #[test]
        fn list_key_entries_includes_doc_comments() {
            let mgr = root_mgr();
            let entries = mgr.key_entries::<tedge_config::TEdgeConfigDto>();
            let mqtt_port = entries.iter().find(|e| e.key == "mqtt.port").unwrap();
            assert_eq!(
                mqtt_port.doc.first().map(|d| d.trim()),
                Some("MQTT broker port")
            );
        }

        #[test]
        fn all_leaf_fields_have_docs() {
            let mgr = root_mgr();
            let entries = mgr.key_entries::<tedge_config::TEdgeConfigDto>();
            for entry in &entries {
                assert!(
                    !entry.doc.is_empty(),
                    "field '{}' has no doc comment",
                    entry.key
                );
            }
        }

        #[test]
        fn list_key_entries_includes_multiple_examples() {
            let mgr = root_mgr();
            let entries = mgr.key_entries::<tedge_config::TEdgeConfigDto>();
            let device_id = entries.iter().find(|e| e.key == "device.id").unwrap();
            assert_eq!(device_id.examples, &["my-device-001", "AINA12345678"]);
        }

        #[test]
        fn list_key_entries_no_examples_returns_empty() {
            let mgr = root_mgr();
            let entries = mgr.key_entries::<tedge_config::TEdgeConfigDto>();
            let mqtt_port = entries.iter().find(|e| e.key == "mqtt.port").unwrap();
            assert!(mqtt_port.examples.is_empty());
        }

        #[test]
        fn all_configs_can_be_retrieved() {
            let mgr = root_mgr();
            let entries = mgr.key_entries::<tedge_config::TEdgeConfigDto>();
            for entry in &entries {
                let value = mgr.get(&tedge_config::TEdgeConfigDto::default(), &entry.key);
                assert!(value.is_ok(), "failed to get key '{}'", entry.key);
            }
        }

        // #[test]
        // fn all_mapper_configs_can_be_retrieved() {
        //     fn read_root(key: &str) -> Option<String> {
        //         let root_mgr = root_mgr();
        //         root_mgr
        //             .read(&tedge_config::TEdgeConfigDto::default(), key)
        //             .unwrap()
        //     };
        //     let mgr = mapper_mgr();
        //     let rdr = mgr
        //         .build_reader::<_, mapper_config::MapperConfig>(
        //             &mapper_config::MapperConfigDto::default(),
        //             Some(&read_root),
        //             "mappers.tb.",
        //         )
        //         .unwrap();
        //     let entries = mgr.key_entries::<mapper_config::MapperConfigDto>();
        //     for entry in &entries {
        //         let value = rdr.read(&entry.key);
        //         dbg!(&entry.key, &value);
        //         assert!(value.is_ok(), "failed to get key '{}'", entry.key);
        //     }
        // }

        #[test]
        fn defaults_not_persisted_in_toml() {
            let dto = tedge_config::TEdgeConfigDto::default();
            let toml_str = toml::to_string_pretty(&dto).unwrap();
            assert!(!toml_str.contains("thin-edge.io"));
            assert!(!toml_str.contains("1883"));
            assert!(!toml_str.contains("localhost"));
        }

        #[test]
        fn toml_round_trip_preserves_all_set_values() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "mqtt.port", "1883").unwrap();
            mgr.set(&mut dto, "device.type", "test-type").unwrap();

            let toml_str = toml::to_string_pretty(&dto).unwrap();
            let dto2: tedge_config::TEdgeConfigDto = toml::from_str(&toml_str).unwrap();

            assert_eq!(mgr.get(&dto2, "mqtt.port").unwrap(), Some("1883".into()));
            assert_eq!(
                mgr.get(&dto2, "device.type").unwrap(),
                Some("test-type".into())
            );
        }

        #[test]
        fn read_returns_static_default_when_unset() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            assert_eq!(
                mgr.read(&dto, "device.type").unwrap(),
                Some("thin-edge.io".into())
            );
            assert_eq!(mgr.read(&dto, "mqtt.port").unwrap(), Some("1883".into()));
        }

        #[test]
        fn read_returns_explicit_value_over_default() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "device.type", "custom-type").unwrap();
            assert_eq!(
                mgr.read(&dto, "device.type").unwrap(),
                Some("custom-type".into())
            );
        }

        #[test]
        fn defaults_adapt_to_config_dir() {
            let mgr = tedge_config::config_manager(Path::new("/custom/config"));
            let dto = tedge_config::TEdgeConfigDto::default();
            assert_eq!(
                mgr.read(&dto, "device.cert_path").unwrap(),
                Some("/custom/config/device-certs/tedge-certificate.pem".into())
            );
            assert_eq!(
                mgr.read(&dto, "device.key_path").unwrap(),
                Some("/custom/config/device-certs/tedge-private-key.pem".into())
            );
        }

        #[test]
        fn deprecated_key_resolves_to_new_key() {
            let mgr = root_mgr();
            let (resolved, deprecated) = mgr.resolve_key("mqtt.external.port");
            assert_eq!(resolved, "mqtt.port");
            assert_eq!(deprecated, Some("mqtt.external.port"));
        }

        #[test]
        fn non_deprecated_key_passes_through() {
            let mgr = root_mgr();
            let (resolved, deprecated) = mgr.resolve_key("mqtt.port");
            assert_eq!(resolved, "mqtt.port");
            assert!(deprecated.is_none());
        }

        #[test]
        fn reader_returns_default_values_when_unset() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            let config: tedge_config::TEdgeConfig = mgr.build_reader(&dto, None, "", None).unwrap();

            assert_eq!(config.mqtt.port, 1883);
            assert_eq!(config.mqtt.host, "localhost");
            assert_eq!(config.device.ty, "thin-edge.io");
        }

        #[test]
        fn reader_returns_none_for_optional_unset_fields() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            let config: tedge_config::TEdgeConfig = mgr.build_reader(&dto, None, "", None).unwrap();

            assert!(config.device.id.or_none().is_none());
        }

        #[test]
        fn reader_picks_up_explicit_values() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "mqtt.port", "9999").unwrap();
            mgr.set(&mut dto, "device.id", "my-device").unwrap();

            let config: tedge_config::TEdgeConfig = mgr.build_reader(&dto, None, "", None).unwrap();

            assert_eq!(config.mqtt.port, 9999);
            assert_eq!(
                config.device.id.or_none().map(String::as_str),
                Some("my-device")
            );
        }

        #[test]
        fn unset_optional_field_error_names_the_key() {
            let mgr = root_mgr();
            let dto = tedge_config::TEdgeConfigDto::default();
            let config: tedge_config::TEdgeConfig = mgr.build_reader(&dto, None, "", None).unwrap();

            let err = config.device.id.or_config_not_set().unwrap_err();
            assert!(err
                .to_string()
                .contains("A value for 'device.id' is missing"));
            assert!(err.to_string().contains("tedge config set device.id"));
        }

        #[test]
        fn optional_field_carries_its_key_when_set() {
            let mgr = root_mgr();
            let mut dto = tedge_config::TEdgeConfigDto::default();
            mgr.set(&mut dto, "device.id", "my-device").unwrap();
            let config: tedge_config::TEdgeConfig = mgr.build_reader(&dto, None, "", None).unwrap();

            assert_eq!(config.device.id.key(), "device.id");
        }
    }

    mod mapper_config_tests {
        use super::*;

        #[test]
        fn set_through_nested_options_initializes_intermediates() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            assert!(dto.proxy.is_none());
            mgr.set(&mut dto, "proxy.bind.port", "9999").unwrap();
            assert!(dto.proxy.is_some());
            assert_eq!(
                mgr.get(&dto, "proxy.bind.port").unwrap(),
                Some("9999".into())
            );
        }

        #[test]
        fn set_and_get_url() {
            let mgr = mapper_mgr();
            let mut dto = mapper_config::MapperConfigDto::default();
            mgr.set(&mut dto, "url", "tenant.cumulocity.com").unwrap();
            assert_eq!(
                mgr.get(&dto, "url").unwrap(),
                Some("tenant.cumulocity.com:443".into())
            );
        }

        #[test]
        fn list_keys_enumerates_all_leaf_fields() {
            let mgr = mapper_mgr();
            let mut keys = mgr.keys::<mapper_config::MapperConfigDto>();
            keys.sort();
            pretty_assertions::assert_eq!(
                keys,
                vec![
                    "cloud_type",
                    "device.cert_path",
                    "device.id",
                    "device.key_path",
                    "url",
                ]
            );
        }

        #[test]
        fn list_key_entries_includes_single_example() {
            let mgr = mapper_mgr();
            let entries = mgr.key_entries::<mapper_config::MapperConfigDto>();
            let url = entries.iter().find(|e| e.key == "url").unwrap();
            assert_eq!(url.examples, &["your-tenant.cumulocity.com"]);
        }

        #[test]
        fn all_leaf_fields_have_docs() {
            let mgr = mapper_mgr();
            let entries = mgr.key_entries::<mapper_config::MapperConfigDto>();
            for entry in &entries {
                assert!(
                    !entry.doc.is_empty(),
                    "field '{}' has no doc comment",
                    entry.key
                );
            }
        }

        #[test]
        fn deeply_nested_set_serializes_only_relevant_groups() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.set(&mut dto, "proxy.bind.port", "9999").unwrap();

            let toml_str = toml::to_string_pretty(&dto).unwrap();

            assert!(toml_str.contains("[proxy.bind]"));
            assert!(toml_str.contains("port = 9999"));
            assert!(!toml_str.contains("[proxy.client]"));
        }

        #[test]
        fn deeply_nested_round_trip_preserves_values() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.set(&mut dto, "proxy.bind.port", "9999").unwrap();
            mgr.set(&mut dto, "proxy.bind.address", "0.0.0.0").unwrap();

            let toml_str = toml::to_string_pretty(&dto).unwrap();
            let dto2: mapper_config::c8y::C8yMapperConfigDto = toml::from_str(&toml_str).unwrap();

            assert_eq!(
                mgr.get(&dto2, "proxy.bind.port").unwrap(),
                Some("9999".into())
            );
            assert_eq!(
                mgr.get(&dto2, "proxy.bind.address").unwrap(),
                Some("0.0.0.0".into())
            );
            assert_eq!(mgr.get(&dto2, "proxy.client.port").unwrap(), None);
        }

        #[test]
        fn toml_round_trip_preserves_url() {
            let mgr = mapper_mgr();
            let mut dto = mapper_config::MapperConfigDto::default();
            mgr.set(&mut dto, "url", "example.com").unwrap();

            let toml_str = toml::to_string_pretty(&dto).unwrap();
            let dto2: mapper_config::MapperConfigDto = toml::from_str(&toml_str).unwrap();

            assert_eq!(
                mgr.get(&dto2, "url").unwrap(),
                Some("example.com:443".into())
            );
        }

        #[test]
        fn from_key_chains_through_defaults() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            assert_eq!(
                mgr.read(&dto, "proxy.client.port").unwrap(),
                Some("8001".into())
            );
        }

        #[test]
        fn read_only_key_rejects_set() {
            let mgr = c8y_mgr();
            assert!(mgr.check_read_only("proxy.client.port").is_err());
        }

        #[test]
        fn writable_key_passes_read_only_check() {
            let mgr = c8y_mgr();
            assert!(mgr.check_read_only("proxy.bind.port").is_ok());
        }

        #[test]
        fn reader_returns_default_values_when_unset() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let config: mapper_config::c8y::C8yMapperConfig =
                mgr.build_reader(&dto, Some(&unset_root), "", None).unwrap();

            assert_eq!(config.proxy.bind.port, 8001);
        }

        #[test]
        fn reader_returns_none_for_optional_unset_fields() {
            let mgr = mapper_mgr();
            let dto = mapper_config::MapperConfigDto::default();
            let config: mapper_config::MapperConfig =
                mgr.build_reader(&dto, Some(&unset_root), "", None).unwrap();

            assert!(config.url.or_none().is_none());
        }

        #[test]
        fn reader_picks_up_explicit_values() {
            let mgr = mapper_mgr();
            let mut dto = mapper_config::MapperConfigDto::default();
            mgr.set(&mut dto, "url", "example.com").unwrap();

            let config: mapper_config::MapperConfig =
                mgr.build_reader(&dto, Some(&unset_root), "", None).unwrap();

            assert_eq!(config.url.or_none().unwrap().to_string(), "example.com:443");
        }

        #[test]
        fn reader_resolves_from_key_to_bind_port_default() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let config: mapper_config::c8y::C8yMapperConfig =
                mgr.build_reader(&dto, Some(&unset_root), "", None).unwrap();

            assert_eq!(config.proxy.client.port, 8001);
        }
    }

    mod c8y_mapper {
        use super::*;

        #[test]
        fn set_templates_set_via_comma_delimited_string() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.set(&mut dto, "smartrest.templates", "t1,t2,t3")
                .unwrap();
            assert_eq!(
                mgr.get(&dto, "smartrest.templates").unwrap(),
                Some("t1,t2,t3".into())
            );
        }

        #[test]
        fn add_appends_values_to_templates_set() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.add(&mut dto, "smartrest.templates", "template1")
                .unwrap();
            mgr.add(&mut dto, "smartrest.templates", "template2")
                .unwrap();
            assert_eq!(
                mgr.get(&dto, "smartrest.templates").unwrap(),
                Some("template1,template2".into())
            );
        }

        #[test]
        fn remove_deletes_matching_value_from_templates_set() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.set(&mut dto, "smartrest.templates", "a,b,c").unwrap();
            mgr.remove(&mut dto, "smartrest.templates", "b").unwrap();
            assert_eq!(
                mgr.get(&dto, "smartrest.templates").unwrap(),
                Some("a,c".into())
            );
        }

        #[test]
        fn has_availability_fields() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            assert_eq!(
                mgr.read(&dto, "availability.enable").unwrap(),
                Some("true".into())
            );
            assert_eq!(
                mgr.read(&dto, "availability.interval").unwrap(),
                Some("3600".into())
            );
        }

        #[test]
        fn has_enable_flags() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            assert_eq!(
                mgr.read(&dto, "enable.log_upload").unwrap(),
                Some("true".into())
            );
        }

        #[test]
        fn keys_include_c8y_specific_fields() {
            let mgr = c8y_mgr();
            let keys = mgr.keys::<mapper_config::c8y::C8yMapperConfigDto>();
            assert!(keys.contains(&"smartrest.templates".into()));
            assert!(keys.contains(&"smartrest.use_operation_id".into()));
            assert!(keys.contains(&"availability.enable".into()));
            assert!(keys.contains(&"availability.interval".into()));
            assert!(keys.contains(&"enable.log_upload".into()));
            assert!(keys.contains(&"url".into()));
        }

        #[test]
        fn generic_mapper_keys_do_not_include_c8y_specific_fields() {
            let mgr = mapper_mgr();
            let keys = mgr.keys::<mapper_config::MapperConfigDto>();
            assert!(!keys.contains(&"smartrest.templates".into()));
            assert!(!keys.contains(&"availability.enable".into()));
        }
    }

    mod key_routing {
        use super::*;

        #[test]
        fn plain_key_unchanged() {
            assert_eq!(resolve_full_key("device.id", None), "device.id");
        }

        #[test]
        fn expands_cloud_alias() {
            assert_eq!(resolve_full_key("c8y.url", None), "mappers.c8y.url");
        }

        #[test]
        fn expands_nested_cloud_alias() {
            assert_eq!(
                resolve_full_key("c8y.proxy.bind.port", None),
                "mappers.c8y.proxy.bind.port"
            );
        }

        #[test]
        fn expands_az_alias() {
            assert_eq!(resolve_full_key("az.url", None), "mappers.az.url");
        }

        #[test]
        fn passes_through_mappers_prefix() {
            assert_eq!(resolve_full_key("mappers.c8y.url", None), "mappers.c8y.url");
        }

        #[test]
        fn appends_profile_for_cloud() {
            assert_eq!(
                resolve_full_key("c8y.url", Some("production")),
                "mappers.c8y.production.url"
            );
        }

        #[test]
        fn profile_applies_to_all_clouds() {
            for alias in ["c8y", "az", "aws"] {
                assert_eq!(
                    resolve_full_key(&format!("{alias}.url"), Some("new")),
                    format!("mappers.{alias}.new.url")
                );
            }
        }

        #[test]
        fn profile_ignored_for_non_cloud() {
            assert_eq!(
                resolve_full_key("mappers.custom.url", Some("production")),
                "mappers.custom.url"
            );
        }

        #[test]
        fn profile_ignored_for_root_keys() {
            assert_eq!(
                resolve_full_key("device.id", Some("production")),
                "device.id"
            );
        }

        #[test]
        fn empty_profile_ignored() {
            assert_eq!(resolve_full_key("c8y.url", Some("")), "mappers.c8y.url");
        }
    }

    mod defaults_validation {
        use super::*;

        #[test]
        fn from_key_rejects_unresolvable_source() {
            let result = DefaultsRegistry::new(vec![FieldDefault {
                key: "a".into(),
                spec: DefaultSpec::FromKey("b".into()),
            }]);
            assert!(result.is_err());
        }
    }

    mod federated {
        use super::*;

        #[test]
        fn read_routes_root_key() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("device.type").unwrap(),
                Some("thin-edge.io".into())
            );
        }

        #[test]
        fn read_routes_mapper_key() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(
                c8y_dir.join("mapper.toml"),
                "url = \"tenant.example.com\"\n",
            )
            .unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.url").unwrap(),
                Some("tenant.example.com:443".into())
            );
        }

        #[test]
        fn list_includes_all_mounts() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            let entries = fed.all_entries();
            let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();

            assert!(keys.contains(&"device.id"));
            assert!(keys.contains(&"mqtt.port"));
            assert!(keys.contains(&"mappers.c8y.url"));
            assert!(keys.contains(&"mappers.c8y.proxy.bind.port"));
        }

        #[test]
        fn mutate_saves_to_correct_file() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let mut fed = build_federated(dir.path(), None, &env).unwrap();

            fed.mutate("mappers.c8y.url", Action::Set("test.com".into()))
                .unwrap();

            let content = std::fs::read_to_string(c8y_dir.join("mapper.toml")).unwrap();
            assert!(content.contains("test.com"));

            let root_content = std::fs::read_to_string(dir.path().join("tedge.toml")).unwrap();
            assert!(!root_content.contains("test.com"));
        }

        #[test]
        fn every_mutation_uses_persisted_values_instead_of_environment_overrides() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(
                dir.path().join("tedge.toml"),
                "[device]\ntype = \"from-file\"\n\n[mqtt]\nport = 1883\n",
            )
            .unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(
                c8y_dir.join("mapper.toml"),
                "[smartrest]\ntemplates = [\"file-a\", \"file-b\"]\n",
            )
            .unwrap();
            let env = EnvOverrides::from_pairs(vec![
                ("TEDGE_MQTT_PORT".into(), "9999".into()),
                ("TEDGE_DEVICE_TYPE".into(), "".into()),
                ("TEDGE_C8Y_SMARTREST_TEMPLATES".into(), "env-a,env-b".into()),
            ]);
            let mut fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(fed.read("mqtt.port").unwrap(), Some("9999".into()));
            assert_eq!(
                fed.read("device.type").unwrap(),
                Some("thin-edge.io".into())
            );
            assert_eq!(
                fed.read("mappers.c8y.smartrest.templates").unwrap(),
                Some("env-a,env-b".into())
            );

            fed.mutate("device.id", Action::Set("temporary".into()))
                .unwrap();
            fed.mutate("device.id", Action::Unset).unwrap();
            fed.mutate(
                "mappers.c8y.smartrest.templates",
                Action::Add("file-c".into()),
            )
            .unwrap();
            fed.mutate(
                "mappers.c8y.smartrest.templates",
                Action::Remove("file-a".into()),
            )
            .unwrap();

            let root: toml::Table = std::fs::read_to_string(dir.path().join("tedge.toml"))
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(root["mqtt"]["port"].as_integer(), Some(1883));
            assert_eq!(root["device"]["type"].as_str(), Some("from-file"));
            assert!(root["device"].get("id").is_none());

            let mapper: toml::Table = std::fs::read_to_string(c8y_dir.join("mapper.toml"))
                .unwrap()
                .parse()
                .unwrap();
            let templates: Vec<_> = mapper["smartrest"]["templates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect();
            assert_eq!(templates, ["file-b", "file-c"]);
        }

        #[test]
        fn discovers_multiple_mappers() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            for name in ["c8y", "az", "custom"] {
                let mapper_dir = dir.path().join("mappers").join(name);
                std::fs::create_dir_all(&mapper_dir).unwrap();
                std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();
            }
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            let entries = fed.all_entries();
            let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();

            assert!(keys.contains(&"mappers.c8y.url"));
            assert!(keys.contains(&"mappers.az.url"));
            assert!(keys.contains(&"mappers.custom.url"));
        }

        #[test]
        fn no_mappers_dir_still_works() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(fed.read("mqtt.port").unwrap(), Some("1883".into()));
            assert!(fed
                .all_entries()
                .iter()
                .any(|e| e.key.starts_with("mappers.c8y.")));
        }

        #[test]
        fn cloud_key_returns_not_set_on_fresh_config_dir() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(fed.read("mappers.c8y.url").unwrap(), None);
        }
    }

    mod env_overrides {
        use super::*;

        #[test]
        fn env_var_overrides_root_value() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![("TEDGE_MQTT_PORT".into(), "9999".into())]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(fed.read("mqtt.port").unwrap(), Some("9999".into()));
        }

        #[test]
        fn cloud_env_var_overrides_mapper_value() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env =
                EnvOverrides::from_pairs(vec![("TEDGE_C8Y_URL".into(), "env.example.com".into())]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.url").unwrap(),
                Some("env.example.com:443".into())
            );
        }

        #[test]
        fn mappers_env_var_overrides_mapper_value() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_MAPPERS_C8Y_URL".into(),
                "mappers-env.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.url").unwrap(),
                Some("mappers-env.example.com:443".into())
            );
        }

        #[test]
        fn mappers_env_var_works_for_custom_mapper() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let custom_dir = dir.path().join("mappers").join("custom");
            std::fs::create_dir_all(&custom_dir).unwrap();
            std::fs::write(custom_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_MAPPERS_CUSTOM_URL".into(),
                "custom.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.custom.url").unwrap(),
                Some("custom.example.com:443".into())
            );
        }

        #[test]
        fn mappers_env_var_works_for_profiled_mapper() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y.staging");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_MAPPERS_C8Y_STAGING_URL".into(),
                "staging.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), Some("staging"), &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.staging.url").unwrap(),
                Some("staging.example.com:443".into())
            );
        }

        #[test]
        fn cloud_env_var_takes_precedence_over_mappers_env_var() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![
                (
                    "TEDGE_MAPPERS_C8Y_URL".into(),
                    "mappers-form.example.com".into(),
                ),
                ("TEDGE_C8Y_URL".into(), "cloud-form.example.com".into()),
            ]);
            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.url").unwrap(),
                Some("cloud-form.example.com:443".into())
            );
        }

        #[test]
        fn profiled_cloud_env_var_overrides_mapper_value() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y.staging");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_C8Y_PROFILES_STAGING_URL".into(),
                "staging.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), Some("staging"), &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.staging.url").unwrap(),
                Some("staging.example.com:443".into())
            );
        }

        #[test]
        fn profiled_cloud_env_var_ignored_for_other_profiles() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y.production");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_C8Y_PROFILES_STAGING_URL".into(),
                "staging.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), Some("production"), &env).unwrap();

            assert_eq!(fed.read("mappers.c8y.production.url").unwrap(), None);
        }

        #[test]
        fn profiled_cloud_env_var_nested_key() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y.prod");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_C8Y_PROFILES_PROD_PROXY_BIND_PORT".into(),
                "9999".into(),
            )]);
            let fed = build_federated(dir.path(), Some("prod"), &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.prod.proxy.bind.port").unwrap(),
                Some("9999".into())
            );
        }

        #[test]
        fn unprofiled_env_var_not_applied_to_profiled_mapper() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y.staging");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env =
                EnvOverrides::from_pairs(vec![("TEDGE_C8Y_URL".into(), "base.example.com".into())]);
            let fed = build_federated(dir.path(), Some("staging"), &env).unwrap();

            assert_eq!(fed.read("mappers.c8y.staging.url").unwrap(), None);
        }

        #[test]
        fn profile_flag_creates_mount_without_directory() {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![(
                "TEDGE_C8Y_PROFILES_STAGING_URL".into(),
                "staging.example.com".into(),
            )]);
            let fed = build_federated(dir.path(), Some("staging"), &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.staging.url").unwrap(),
                Some("staging.example.com:443".into())
            );
        }
    }

    mod from_root_defaults {
        use super::*;

        #[test]
        fn fills_mapper_cert_from_root() {
            let dir = tempfile::tempdir().unwrap();
            let root_toml =
                "[device]\ncert_path = \"/custom/cert.pem\"\nkey_path = \"/custom/key.pem\"\n";
            std::fs::write(dir.path().join("tedge.toml"), root_toml).unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(c8y_dir.join("mapper.toml"), "").unwrap();
            let env = EnvOverrides::from_pairs(vec![]);

            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.device.cert_path").unwrap(),
                Some("/custom/cert.pem".into())
            );
        }

        #[test]
        fn does_not_overwrite_explicit_mapper_cert() {
            let dir = tempfile::tempdir().unwrap();
            let root_toml =
                "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n";
            std::fs::write(dir.path().join("tedge.toml"), root_toml).unwrap();
            let c8y_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&c8y_dir).unwrap();
            std::fs::write(
                c8y_dir.join("mapper.toml"),
                "[device]\ncert_path = \"/c8y/cert.pem\"\n",
            )
            .unwrap();
            let env = EnvOverrides::from_pairs(vec![]);

            let fed = build_federated(dir.path(), None, &env).unwrap();

            assert_eq!(
                fed.read("mappers.c8y.device.cert_path").unwrap(),
                Some("/c8y/cert.pem".into())
            );
        }

        #[test]
        fn resolved_via_build_reader_with_root() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let config: mapper_config::c8y::C8yMapperConfig = mgr
                .build_reader(
                    &dto,
                    Some(&|key| {
                        Ok(match key {
                            "device.cert_path" => Some("/nonexistent/root/cert.pem".into()),
                            "device.key_path" => Some("/nonexistent/root/key.pem".into()),
                            _ => None,
                        })
                    }),
                    "",
                    None,
                )
                .unwrap();

            assert_eq!(
                config.device.cert_path.or_none().unwrap().as_str(),
                "/nonexistent/root/cert.pem"
            );
            assert_eq!(
                config.device.key_path.or_none().unwrap().as_str(),
                "/nonexistent/root/key.pem"
            );
        }

        #[test]
        fn explicit_value_overrides_root_in_reader() {
            let mgr = c8y_mgr();
            let mut dto = mapper_config::c8y::C8yMapperConfigDto::default();
            mgr.set(&mut dto, "device.cert_path", "/c8y/cert.pem")
                .unwrap();
            let config: mapper_config::c8y::C8yMapperConfig = mgr
                .build_reader(
                    &dto,
                    Some(&|key| {
                        Ok(match key {
                            "device.cert_path" => Some("/nonexistent/root/cert.pem".into()),
                            "device.key_path" => Some("/nonexistent/root/key.pem".into()),
                            _ => None,
                        })
                    }),
                    "",
                    None,
                )
                .unwrap();

            assert_eq!(
                config.device.cert_path.or_none().unwrap().as_str(),
                "/c8y/cert.pem"
            );
            assert_eq!(
                config.device.key_path.or_none().unwrap().as_str(),
                "/nonexistent/root/key.pem"
            );
        }

        #[test]
        fn read_without_root_config_is_an_error() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let err = mgr.read(&dto, "device.cert_path").unwrap_err();
            assert_eq!(
                err.to_string(),
                "'device.cert_path' can fall back to the root config key 'device.cert_path', but no root config was supplied"
            );
        }

        #[test]
        fn build_reader_without_root_config_is_an_error() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let err = mgr
                .build_reader::<_, mapper_config::c8y::C8yMapperConfig>(&dto, None, "", None)
                .unwrap_err();
            assert!(
                err.to_string().contains("no root config was supplied"),
                "unexpected error: {err}"
            );
        }

        #[test]
        fn nested_optional_field_key_is_dotted() {
            let mgr = c8y_mgr();
            let dto = mapper_config::c8y::C8yMapperConfigDto::default();
            let config: mapper_config::c8y::C8yMapperConfig =
                mgr.build_reader(&dto, Some(&unset_root), "", None).unwrap();

            assert_eq!(config.device.cert_path.key(), "device.cert_path");
        }

        #[test]
        fn generic_mapper_struct_access() {
            let mgr = mapper_mgr();
            let dto = mapper_config::MapperConfigDto::default();
            let config: mapper_config::MapperConfig = mgr
                .build_reader(
                    &dto,
                    Some(&|key| {
                        Ok(match key {
                            "device.cert_path" => Some("/nonexistent/root/cert.pem".into()),
                            "device.key_path" => Some("/nonexistent/root/key.pem".into()),
                            _ => None,
                        })
                    }),
                    "",
                    None,
                )
                .unwrap();

            assert_eq!(
                config.device.cert_path.or_none().unwrap().as_str(),
                "/nonexistent/root/cert.pem"
            );
            assert_eq!(
                config.device.key_path.or_none().unwrap().as_str(),
                "/nonexistent/root/key.pem"
            );
        }

        #[test]
        fn not_persisted_in_toml() {
            let dto = mapper_config::MapperConfigDto::default();
            let toml_str = toml::to_string_pretty(&dto).unwrap();
            assert!(
                !toml_str.contains("cert_path"),
                "from_root defaults should not appear in serialized TOML"
            );
        }
    }

    mod loaded_mapper_dispatch {
        use mapper_config::CloudSchema;

        #[test]
        fn cloud_type_c8y_returns_c8y_variant() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("mycloud");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "cloud_type = \"c8y\"\n").unwrap();

            let loaded = mapper_config::load(dir.path(), "mycloud", None, root(&dir).as_ref()).unwrap();
            assert!(
                matches!(loaded, mapper_config::LoadedMapper::C8y(_)),
                "expected C8y variant, got {loaded:?}"
            );
        }

        #[test]
        fn without_cloud_type_returns_custom_variant() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("foo");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();

            let loaded = mapper_config::load(dir.path(), "foo", None, root(&dir).as_ref()).unwrap();
            assert!(
                matches!(loaded, mapper_config::LoadedMapper::Custom(_)),
                "expected Custom variant, got {loaded:?}"
            );
        }

        #[test]
        fn c8y_variant_has_smartrest_defaults() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("mycloud");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "cloud_type = \"c8y\"\n").unwrap();

            let mapper_config::LoadedMapper::C8y(config) =
                mapper_config::load(dir.path(), "mycloud", None, root(&dir).as_ref()).unwrap()
            else {
                panic!("expected C8y variant");
            };
            assert!(config.smartrest.use_operation_id);
        }

        #[test]
        fn custom_variant_has_cloud_type_field() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("foo");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();

            let mapper_config::LoadedMapper::Custom(config) =
                mapper_config::load(dir.path(), "foo", None, root(&dir).as_ref()).unwrap()
            else {
                panic!("expected Custom variant");
            };
            assert_eq!(config.cloud_type, CloudSchema::Custom);
        }

        #[test]
        fn builtin_name_c8y_without_explicit_cloud_type_returns_c8y() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();

            let loaded = mapper_config::load(dir.path(), "c8y", None, root(&dir).as_ref()).unwrap();
            assert!(
                matches!(loaded, mapper_config::LoadedMapper::C8y(_)),
                "expected C8y variant for builtin name, got {loaded:?}"
            );
        }

        #[test]
        fn loaded_mapper_error_names_full_user_facing_key() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("c8y");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();

            let mapper_config::LoadedMapper::C8y(config) =
                mapper_config::load(dir.path(), "c8y", None, root(&dir).as_ref()).unwrap()
            else {
                panic!("expected C8y variant");
            };
            let err = config.url.or_config_not_set().unwrap_err();
            assert!(err.to_string().contains("A value for 'c8y.url' is missing"));
            assert!(err.to_string().contains("tedge config set c8y.url"));
        }

        #[test]
        fn profiled_mapper_error_includes_profile() {
            let dir = tempfile::TempDir::new().unwrap();
            let mapper_dir = dir.path().join("mappers").join("c8y.staging");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "").unwrap();

            let mapper_config::LoadedMapper::C8y(config) = mapper_config::load(
                dir.path(),
                "c8y",
                Some("staging"),
                root(&dir).as_ref(),
            )
            .unwrap()
            else {
                panic!("expected C8y variant");
            };
            let err = config.url.or_config_not_set().unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("A value for 'c8y.url' is missing (profile 'staging')"),
                "unexpected error: {msg}"
            );
            assert!(
                msg.contains("--profile staging"),
                "expected --profile hint: {msg}"
            );
        }

        fn root(dir: &tempfile::TempDir) -> Box<dyn facet_config_runtime::ops::ConfigOps> {
            let env = facet_config_runtime::EnvOverrides::from_pairs(vec![]);
            tedge_config::source(dir.path(), &env).unwrap()
        }
    }

    // A config dir that never exists on the host, so defaults derived from
    // files (such as device.id from the certificate) stay unset in tests
    const TEST_CONFIG_DIR: &str = "/nonexistent/tedge";

    fn root_mgr() -> facet_config_runtime::ConfigManager {
        tedge_config::config_manager(Path::new(TEST_CONFIG_DIR))
    }

    fn mapper_mgr() -> facet_config_runtime::ConfigManager {
        mapper_config::config_manager(Path::new(TEST_CONFIG_DIR))
    }

    fn c8y_mgr() -> facet_config_runtime::ConfigManager {
        mapper_config::c8y::config_manager(Path::new(TEST_CONFIG_DIR))
    }

    // A root config with every key unset, for tests that need a root config
    // present but don't care about its values
    fn unset_root(_key: &str) -> Result<Option<String>, facet_config_runtime::ConfigError> {
        Ok(None)
    }
}
