//! Custom mapper component.
//!
//! A custom mapper is started with `tedge-mapper custom [--profile <name>]`. It reads
//! its configuration from the mapper directory (`{config_dir}/mappers/custom.{name}/`)
//! and conditionally starts the built-in MQTT bridge and/or flows engine based on what
//! files are present in that directory.

use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::custom::config::load_mapper_config;
use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::tedge_toml::MqttAuthClientConfigCloudBroker;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::PrivateKeyType;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::ConnectedFlowRegistry;
use tedge_flows::FlowsMapperBuilder;
use tedge_flows::FlowsMapperConfig;
use tedge_mqtt_bridge::config_toml::AuthMethod;
use tedge_mqtt_bridge::load_bridge_rules_from_directory;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_mqtt_bridge::MqttOptions;
use tedge_watch_ext::WatchActorBuilder;
use tracing::warn;

/// A custom mapper instance, identified by an optional profile name.
///
/// The mapper directory is `{config_dir}/mappers/custom.{profile}/` (or `custom/` when
/// no profile is given).
pub struct CustomMapper {
    pub profile: Option<ProfileName>,
}

impl CustomMapper {
    /// Returns the mapper directory path for this profile.
    ///
    /// - `custom.{name}/` when a profile is provided
    /// - `custom/` when no profile is given
    pub fn mapper_dir(&self, config_dir: &Utf8Path) -> Utf8PathBuf {
        let dir_name = match &self.profile {
            None => "custom".to_string(),
            Some(profile) => format!("custom.{profile}"),
        };
        config_dir.join("mappers").join(dir_name)
    }

    /// Returns the service name for this mapper instance.
    ///
    /// - `tedge-mapper-custom@{profile}` when a profile is provided
    /// - `tedge-mapper-custom` when no profile is given
    pub fn service_name(&self) -> String {
        match &self.profile {
            None => "tedge-mapper-custom".to_string(),
            Some(profile) => format!("tedge-mapper-custom@{profile}"),
        }
    }

    /// Returns the bridge service name for this mapper instance.
    ///
    /// - `tedge-mapper-bridge-custom@{profile}` when a profile is provided
    /// - `tedge-mapper-bridge-custom` when no profile is given
    pub fn bridge_service_name(&self) -> String {
        match &self.profile {
            None => "tedge-mapper-bridge-custom".to_string(),
            Some(profile) => format!("tedge-mapper-bridge-custom@{profile}"),
        }
    }
}

/// Validates that the mapper directory for the given profile exists.
///
/// If the profile directory is missing, returns an error listing available
/// `custom.*` profiles found in the mappers directory.
pub fn validate_profile_dir(mapper_dir: &Utf8Path, config_dir: &Utf8Path) -> anyhow::Result<()> {
    if mapper_dir.exists() {
        return Ok(());
    }

    // Collect available custom profiles to help the user
    let mappers_dir = config_dir.join("mappers");
    let available = list_custom_profiles(&mappers_dir);

    if available.is_empty() {
        anyhow::bail!(
            "Custom mapper directory '{}' does not exist. \
             No custom mapper profiles found under '{}'.",
            mapper_dir,
            mappers_dir
        );
    } else {
        anyhow::bail!(
            "Custom mapper directory '{}' does not exist. \
             Available custom mapper profiles: {}",
            mapper_dir,
            available.join(", ")
        );
    }
}

/// Lists available custom mapper profiles by scanning the mappers directory.
///
/// Returns profile names (the part after `custom.`) for each `custom.{name}/`
/// directory found, plus an empty string for `custom/` if it exists.
fn list_custom_profiles(mappers_dir: &Utf8Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(mappers_dir) else {
        return Vec::new();
    };

    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        if name == "custom" {
            profiles.push("(default, no --profile)".to_string());
        } else if let Some(profile) = name.strip_prefix("custom.") {
            profiles.push(format!("--profile {profile}"));
        }
    }
    profiles.sort();
    profiles
}

#[async_trait]
impl TEdgeComponent for CustomMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &tedge_config::Path,
    ) -> anyhow::Result<()> {
        let mapper_dir = self.mapper_dir(config_dir);
        let service_name = self.service_name();

        // 3.3: Validate that the mapper directory exists
        validate_profile_dir(&mapper_dir, config_dir)?;

        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&service_name, &tedge_config).await?;

        let has_bridge_dir = mapper_dir.join("bridge").is_dir();
        let has_flows_dir = mapper_dir.join("flows").is_dir();

        // 4.3/4.4: Check for bridge directory without connection config
        if has_bridge_dir {
            let mapper_config = load_mapper_config(&mapper_dir).await?;

            match mapper_config {
                None => {
                    anyhow::bail!(
                        "Mapper directory '{}' contains a 'bridge/' subdirectory but no \
                         'tedge.toml' connection config. \
                         Create a tedge.toml with [connection] settings to use the MQTT bridge.",
                        mapper_dir
                    );
                }
                Some(config) => {
                    // 4.5: Start the MQTT bridge
                    let bridge_dir = mapper_dir.join("bridge");
                    let bridge_service_name = self.bridge_service_name();
                    let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
                    let device_topic_id = tedge_config.mqtt.device_topic_id.clone();
                    let health_topic =
                        service_health_topic(&mqtt_schema, &device_topic_id, &bridge_service_name);

                    let conn = config.connection.as_ref().with_context(|| {
                        format!(
                            "'{}/tedge.toml' is missing a [connection] section required \
                             for bridge rules",
                            mapper_dir
                        )
                    })?;

                    let mut cloud_config = MqttOptions::new(&service_name, &conn.url, conn.port);
                    cloud_config.set_clean_session(false);

                    // Set up certificate authentication if cert/key paths are provided
                    if let Some(device) = &config.device {
                        if let (Some(cert_path), Some(key_path)) =
                            (&device.cert_path, &device.key_path)
                        {
                            let ca_path = device
                                .root_cert_path
                                .clone()
                                .unwrap_or_else(|| Utf8PathBuf::from("/etc/ssl/certs"));

                            let tls_config = MqttAuthConfigCloudBroker {
                                ca_path,
                                client: Some(MqttAuthClientConfigCloudBroker {
                                    cert_file: cert_path.clone(),
                                    private_key: PrivateKeyType::File(key_path.clone()),
                                }),
                            }
                            .to_rustls_client_config()
                            .context("Failed to create MQTT TLS config")?;
                            cloud_config
                                .set_transport(Transport::tls_with_config(tls_config.into()));
                        }
                    }

                    configure_proxy(&tedge_config, &mut cloud_config)?;

                    let bridge_rules = load_bridge_rules_from_directory(
                        &bridge_dir,
                        &tedge_config,
                        AuthMethod::Certificate,
                        None,
                        Some(&config.table),
                    )
                    .await?;

                    let bridge_actor = MqttBridgeActorBuilder::new(
                        &tedge_config,
                        &bridge_service_name,
                        &health_topic,
                        bridge_rules,
                        cloud_config,
                        None,
                    )
                    .await;
                    runtime.spawn(bridge_actor).await?;
                }
            }
        }

        // 4.6: Start the flows engine if flows/ is present
        if has_flows_dir {
            let service_topic_id = EntityTopicId::default_main_service(&service_name)?;
            let te = &tedge_config.mqtt.topic_root;
            let stats_config = &tedge_config.flows.stats;
            let service_config = FlowsMapperConfig::new(
                &format!("{te}/{service_topic_id}"),
                stats_config.interval.duration(),
                stats_config.on_message,
                stats_config.on_interval,
            );

            let flows_dir = mapper_dir.join("flows");
            let flows = ConnectedFlowRegistry::new(flows_dir);

            let mut fs_actor = FsWatchActorBuilder::new();
            let mut cmd_watcher_actor = WatchActorBuilder::new();
            let mut flows_mapper = FlowsMapperBuilder::try_new(flows, service_config).await?;
            flows_mapper.connect(&mut mqtt_actor);
            flows_mapper.connect_fs(&mut fs_actor);
            flows_mapper.connect_cmd(&mut cmd_watcher_actor);

            runtime.spawn(flows_mapper).await?;
            runtime.spawn(fs_actor).await?;
            runtime.spawn(cmd_watcher_actor).await?;
        }

        // 4.7: If neither bridge nor flows — start successfully with no active components
        if !has_bridge_dir && !has_flows_dir {
            warn!(
                "Custom mapper '{}' has no 'bridge/' or 'flows/' directory — \
                 starting with no active components",
                mapper_dir
            );
        }

        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod service_names {
        use super::*;

        #[test]
        fn without_profile() {
            let mapper = CustomMapper { profile: None };
            assert_eq!(mapper.service_name(), "tedge-mapper-custom");
            assert_eq!(mapper.bridge_service_name(), "tedge-mapper-bridge-custom");
        }

        #[test]
        fn with_profile() {
            let mapper = CustomMapper {
                profile: Some("thingsboard".parse().unwrap()),
            };
            assert_eq!(mapper.service_name(), "tedge-mapper-custom@thingsboard");
            assert_eq!(
                mapper.bridge_service_name(),
                "tedge-mapper-bridge-custom@thingsboard"
            );
        }
    }

    mod mapper_dir {
        use super::*;

        #[test]
        fn without_profile_uses_custom_dir() {
            let mapper = CustomMapper { profile: None };
            let dir = mapper.mapper_dir(Utf8Path::new("/etc/tedge"));
            assert_eq!(dir, Utf8PathBuf::from("/etc/tedge/mappers/custom"));
        }

        #[test]
        fn with_profile_uses_prefixed_dir() {
            let mapper = CustomMapper {
                profile: Some("thingsboard".parse().unwrap()),
            };
            let dir = mapper.mapper_dir(Utf8Path::new("/etc/tedge"));
            assert_eq!(
                dir,
                Utf8PathBuf::from("/etc/tedge/mappers/custom.thingsboard")
            );
        }
    }

    mod profile_validation {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[test]
        fn succeeds_when_directory_exists() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/custom.test");
            std::fs::create_dir_all(&mapper_dir).unwrap();

            assert!(validate_profile_dir(&mapper_dir, config_dir).is_ok());
        }

        #[test]
        fn errors_when_directory_missing() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/custom.nonexistent");

            let err = validate_profile_dir(&mapper_dir, config_dir).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention directory missing: {msg}"
            );
        }

        #[test]
        fn error_lists_available_profiles() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();

            // Create some existing custom profiles
            std::fs::create_dir_all(config_dir.join("mappers/custom.thingsboard")).unwrap();
            std::fs::create_dir_all(config_dir.join("mappers/custom.mycloud")).unwrap();

            let mapper_dir = config_dir.join("mappers/custom.nonexistent");
            let err = validate_profile_dir(&mapper_dir, config_dir).unwrap_err();
            let msg = format!("{err}");

            assert!(
                msg.contains("thingsboard") || msg.contains("mycloud"),
                "Error should list available profiles: {msg}"
            );
        }

        #[test]
        fn error_when_no_profiles_exist() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            // Create mappers dir but no custom profiles
            std::fs::create_dir_all(config_dir.join("mappers")).unwrap();

            let mapper_dir = config_dir.join("mappers/custom.nonexistent");
            let err = validate_profile_dir(&mapper_dir, config_dir).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention missing directory: {msg}"
            );
        }
    }
}
