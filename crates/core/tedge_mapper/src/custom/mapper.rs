//! Custom mapper component.
//!
//! A user-defined mapper is started with `tedge-mapper <name>`. It reads its configuration
//! from the mapper directory (`{config_dir}/mappers/{name}/`) and conditionally starts the
//! built-in MQTT bridge and/or flows engine based on what files are present.

use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use crate::core::mqtt::configure_proxy;
use crate::custom::config::load_mapper_config;
use crate::custom::config::read_mapper_credentials;
use crate::custom::config::scan_mappers_shallow;
use crate::custom::config::BridgeTlsEnable;
use crate::custom::config::CustomMapperConfig;
use crate::custom::resolve::resolve_effective_config;
use crate::custom::resolve::EffectiveMapperConfig;
use anyhow::bail;
use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_actors::MessageSink;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::tedge_toml::MqttAuthClientConfigCloudBroker;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::PrivateKeyType;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::FlowsMapperBuilder;
use tedge_flows::FlowsMapperConfig;
use tedge_mqtt_bridge::config_toml::AuthMethod;
use tedge_mqtt_bridge::load_bridge_rules_from_directory;
use tedge_mqtt_bridge::rumqttc::Transport;
use tedge_mqtt_bridge::use_credentials;
use tedge_mqtt_bridge::MqttBridgeActorBuilder;
use tedge_mqtt_bridge::MqttOptions;
use tedge_watch_ext::WatchActorBuilder;

/// A user-defined mapper instance, identified by its name.
///
/// The mapper directory is `{config_dir}/mappers/{name}/`.
pub struct CustomMapper {
    pub name: String,
}

impl CustomMapper {
    /// Returns the mapper directory path for this instance.
    pub fn mapper_dir(&self, config_dir: &Utf8Path) -> Utf8PathBuf {
        config_dir.join("mappers").join(&self.name)
    }

    /// Returns the service name: `tedge-mapper-{name}`.
    pub fn service_name(&self) -> String {
        format!("tedge-mapper-{}", self.name)
    }

    /// Returns the bridge service name: `tedge-mapper-bridge-{name}`.
    pub fn bridge_service_name(&self) -> String {
        format!("tedge-mapper-bridge-{}", self.name)
    }
}

/// The result of validating and loading a mapper directory at startup.
///
/// This is the single authoritative description of what a mapper directory
/// contains and what components should be started. Produced by
/// [`validate_and_load`], then used directly to drive [`start`].
#[derive(Debug)]
pub enum MapperStartup {
    /// The mapper directory has no `bridge/` subdirectory.
    /// Only the flows engine will be started; no cloud MQTT connection.
    /// The `flows/` directory is created automatically on startup if it
    /// does not exist.
    FlowsOnly,
    /// The mapper directory has a `bridge/` subdirectory (with a valid
    /// `mapper.toml`). The MQTT bridge will be started. The flows engine
    /// is also always started; the `flows/` directory is created
    /// automatically on startup if it does not exist.
    WithBridge { config: Box<CustomMapperConfig> },
}

/// Validates the mapper directory and loads its configuration in one step.
///
/// This is the single point of startup validation for a user-defined mapper.
/// The validation sequence is:
/// 1. The mapper directory must exist.
/// 2. If `bridge/` is present, `mapper.toml` must exist and be valid.
///
/// If `bridge/` is absent, [`MapperStartup::FlowsOnly`] is returned — the
/// flows engine will always be started. The `flows/` directory is created
/// automatically by [`build_flows_actors`] if it does not exist.
///
/// Returns a typed [`MapperStartup`] that describes what should be started,
/// eliminating the need for callers to re-inspect the directory state.
pub async fn validate_and_load(
    mapper_dir: &Utf8Path,
    config_dir: &Utf8Path,
) -> anyhow::Result<MapperStartup> {
    // 1. Directory must exist.
    if !tokio::fs::try_exists(mapper_dir).await.unwrap_or(false)
        || !tokio::fs::metadata(mapper_dir)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    {
        let mappers_root = config_dir.join("mappers");
        let available = list_available_mappers(&mappers_root).await;
        if available.is_empty() {
            anyhow::bail!(
                "Mapper directory '{mapper_dir}' does not exist. \
                 No mappers found under '{mappers_root}'.",
            );
        } else {
            anyhow::bail!(
                "Mapper directory '{mapper_dir}' does not exist. \
                 Available mappers: {}",
                available.join(", ")
            );
        }
    }

    let has_bridge_dir = mapper_dir.join("bridge").is_dir();

    if !has_bridge_dir {
        return Ok(MapperStartup::FlowsOnly);
    }

    // 2. bridge/ present — mapper.toml is required for connection settings.
    let config = load_mapper_config(mapper_dir).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "Mapper directory '{mapper_dir}' contains a 'bridge/' subdirectory but no \
             'mapper.toml' connection config. \
             Create a mapper.toml with a top-level 'url' field (e.g. url = \"host:8883\") to use the MQTT bridge."
        )
    })?;

    Ok(MapperStartup::WithBridge {
        config: Box::new(config),
    })
}

/// Lists available mappers by scanning the mappers root directory for subdirectories.
async fn list_available_mappers(mappers_root: &Utf8Path) -> Vec<String> {
    scan_mappers_shallow(mappers_root)
        .await
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// Builds the [`MqttOptions`] for the cloud broker connection described by `config`.
///
/// Accepts an already-resolved [`EffectiveMapperConfig`] (produced by
/// [`resolve_effective_config`]) and handles only MQTT wiring: TLS setup or
/// password credentials, client ID, keepalive, and proxy. Resolution of cert/key
/// paths, auth method, and device identity is done upstream in
/// `resolve_effective_config`.
///
/// Returns `(MqttOptions, AuthMethod)` so the caller can pass `AuthMethod` to
/// `load_bridge_rules_from_directory`.
pub async fn build_cloud_mqtt_options(
    config: &EffectiveMapperConfig,
    service_name: &str,
    mapper_dir: &Utf8Path,
    tedge_config: &TEdgeConfig,
) -> anyhow::Result<(MqttOptions, AuthMethod)> {
    let url = config.url.as_ref().map(|s| &s.value).with_context(|| {
        format!("'{mapper_dir}/mapper.toml' is missing a 'url' field required for the MQTT bridge")
    })?;

    let mut cloud_config = MqttOptions::new(service_name, url.host().to_string(), url.port().0);
    cloud_config.set_clean_session(config.bridge.clean_session);
    if let Some(interval) = &config.bridge.keepalive_interval {
        cloud_config.set_keep_alive(interval.duration());
    }

    let tls_enabled = match config.bridge.tls.enable {
        BridgeTlsEnable::True => true,
        BridgeTlsEnable::False => false,
        BridgeTlsEnable::Auto => match url.port().0 {
            1883 => false,
            _ => true, // 8883 and any other port default to TLS on
        },
    };

    let ca_path = &config.root_cert_path.value;

    if !tls_enabled && config.effective_auth.value == AuthMethod::Certificate {
        anyhow::bail!(
            "certificate authentication requires TLS, but TLS is disabled for this mapper. \
             Either set `[bridge]\ntls = \"on\"` in mapper.toml, or change the auth method."
        );
    }

    match config.effective_auth.value {
        AuthMethod::Certificate => {
            let cert_path = config
                .cert_path
                .as_ref()
                .map(|s| &s.value)
                .with_context(|| {
                    format!(
                        "'{mapper_dir}/mapper.toml' requires a device certificate \
                         for certificate authentication"
                    )
                })?;
            let key_path = config
                .key_path
                .as_ref()
                .map(|s| &s.value)
                .with_context(|| {
                    format!(
                        "'{mapper_dir}/mapper.toml' requires a device private key \
                         for certificate authentication"
                    )
                })?;
            if tls_enabled {
                let tls_config = MqttAuthConfigCloudBroker {
                    ca_path: ca_path.clone(),
                    client: Some(MqttAuthClientConfigCloudBroker {
                        cert_file: cert_path.clone(),
                        private_key: PrivateKeyType::File(key_path.clone()),
                    }),
                }
                .to_rustls_client_config()
                .context("Failed to create MQTT TLS config")?;
                cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));
            }
        }
        AuthMethod::Password => {
            let creds_path = config
                .credentials_path
                .as_ref()
                .map(|s| s.value.as_path())
                .with_context(|| {
                    format!(
                        "'{mapper_dir}/mapper.toml' sets auth_method = \"password\" but \
                         no credentials_path is configured"
                    )
                })?;
            let (username, password) = read_mapper_credentials(creds_path).await?;
            if tls_enabled {
                use_credentials(&mut cloud_config, ca_path, username, password)?;
            } else {
                cloud_config.set_credentials(username, password);
            }
        }
    }

    let device_id = config
        .device_id
        .as_ref()
        .map(|s| s.value.clone())
        .with_context(|| {
            format!(
                "No MQTT client ID could be determined: configure 'device.id' in \
                 '{mapper_dir}/mapper.toml'"
            )
        })?;
    cloud_config.set_client_id(device_id);

    configure_proxy(tedge_config, &mut cloud_config)?;

    Ok((cloud_config, config.effective_auth.value))
}

/// Constructs the flows-mapper builder and its supporting file-watch actors.
///
/// Returns `(FlowsMapperBuilder, FsWatchActorBuilder, WatchActorBuilder)`.
/// The caller is responsible for wiring the actors together (`.connect`, `.connect_fs`,
/// `.connect_cmd`) and spawning them into the runtime — keeping the actor-graph
/// wiring visible at the `start` level rather than buried inside a helper.
async fn build_flows_actors(
    mapper_dir: &Utf8Path,
    service_name: &str,
    tedge_config: &TEdgeConfig,
) -> anyhow::Result<(FlowsMapperBuilder, FsWatchActorBuilder, WatchActorBuilder)> {
    let Some(service_topic_id) = &tedge_config
        .mqtt
        .device_topic_id
        .default_service_for_device(service_name)
    else {
        bail!(
            "Unknown topic id for {service_name} on {}",
            tedge_config.mqtt.device_topic_id
        );
    };
    let te = &tedge_config.mqtt.topic_root;
    let stats_config = &tedge_config.flows.stats;
    let service_config = FlowsMapperConfig::new(
        &format!("{te}/{service_topic_id}"),
        stats_config.interval.duration(),
        stats_config.on_message,
        stats_config.on_interval,
        stats_config.on_startup,
    );

    let flows_dir = mapper_dir.join("flows");
    let mapper_config = crate::effective_mapper_config(
        tedge_config,
        mapper_dir.file_name().unwrap_or("local"),
        mapper_dir,
    )
    .await?;
    let flows = crate::flow_registry(mapper_config, flows_dir).await?;
    let fs_actor = FsWatchActorBuilder::new();
    let cmd_watcher_actor = WatchActorBuilder::new();
    let flows_mapper = FlowsMapperBuilder::try_new(flows, service_config).await?;

    Ok((flows_mapper, fs_actor, cmd_watcher_actor))
}

/// Builds the empty retained message used to clear a stale bridge service
/// health status when the mapper starts in flows-only mode.
fn bridge_health_clear_message(
    mapper: &CustomMapper,
    tedge_config: &TEdgeConfig,
) -> mqtt_channel::MqttMessage {
    let bridge_service_name = mapper.bridge_service_name();
    let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
    let device_topic_id = tedge_config.mqtt.device_topic_id.clone();
    let health_topic = service_health_topic(&mqtt_schema, &device_topic_id, &bridge_service_name);
    mqtt_channel::MqttMessage::new(&health_topic, vec![]).with_retain()
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

        let startup = validate_and_load(&mapper_dir, config_dir).await?;

        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&service_name, &tedge_config).await?;

        if let MapperStartup::WithBridge { ref config, .. } = startup {
            let bridge_dir = mapper_dir.join("bridge");
            let bridge_service_name = self.bridge_service_name();
            let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
            let device_topic_id = tedge_config.mqtt.device_topic_id.clone();
            let health_topic =
                service_health_topic(&mqtt_schema, &device_topic_id, &bridge_service_name);

            let effective = resolve_effective_config(config, &tedge_config, None, None).await?;
            let (cloud_config, effective_auth) =
                build_cloud_mqtt_options(&effective, &service_name, &mapper_dir, &tedge_config)
                    .await?;

            let bridge_rules = load_bridge_rules_from_directory(
                &bridge_dir,
                &tedge_config,
                effective_auth,
                None,
                &effective,
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
        } else {
            // Flows-only mode: clear any stale retained bridge health message
            // from a previous run that had a bridge configured.
            let clear_msg = bridge_health_clear_message(self, &tedge_config);
            let mut sender = mqtt_actor.get_sender();
            // Best-effort: if the MQTT actor has already shut down the error is harmless.
            let _ = sender.send(clear_msg).await;
        }

        let (mut flows_mapper, mut fs_actor, mut cmd_watcher_actor) =
            build_flows_actors(&mapper_dir, &service_name, &tedge_config).await?;
        flows_mapper.connect(&mut mqtt_actor);
        flows_mapper.connect_fs(&mut fs_actor);
        flows_mapper.connect_cmd(&mut cmd_watcher_actor);

        runtime.spawn(flows_mapper).await?;
        runtime.spawn(fs_actor).await?;
        runtime.spawn(cmd_watcher_actor).await?;

        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod flows_only_deregistration {
        use super::*;
        use tedge_config::TEdgeConfig;

        #[test]
        fn flows_only_startup_publishes_empty_retained_message_to_bridge_health_topic() {
            let mapper = CustomMapper {
                name: "thingsboard".to_string(),
            };
            let tedge_config = TEdgeConfig::load_toml_str("");
            let msg = bridge_health_clear_message(&mapper, &tedge_config);
            assert_eq!(
                msg.topic.name,
                "te/device/main/service/tedge-mapper-bridge-thingsboard/status/health"
            );
            assert!(
                msg.payload_bytes().is_empty(),
                "payload should be empty to clear the retained message"
            );
            assert!(msg.retain, "message must be retained");
        }
    }

    mod service_names {
        use super::*;

        #[test]
        fn uses_mapper_name_in_service_names() {
            let mapper = CustomMapper {
                name: "thingsboard".to_string(),
            };
            assert_eq!(mapper.service_name(), "tedge-mapper-thingsboard");
            assert_eq!(
                mapper.bridge_service_name(),
                "tedge-mapper-bridge-thingsboard"
            );
        }
    }

    mod mapper_dir {
        use super::*;

        #[test]
        fn uses_name_in_no_prefix_dir() {
            let mapper = CustomMapper {
                name: "thingsboard".to_string(),
            };
            let dir = mapper.mapper_dir(Utf8Path::new("/etc/tedge"));
            assert_eq!(dir, Utf8PathBuf::from("/etc/tedge/mappers/thingsboard"));
        }
    }

    mod validate_and_load {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        /// Directory does not exist → error mentioning the missing directory.
        #[tokio::test]
        async fn errors_when_directory_missing() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/nonexistent");

            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention directory missing: {msg}"
            );
        }

        /// Error when directory is missing lists available mappers.
        #[tokio::test]
        async fn error_lists_available_mappers() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();

            for name in ["thingsboard", "mycloud"] {
                let dir = config_dir.join(format!("mappers/{name}"));
                tokio::fs::create_dir_all(&dir).await.unwrap();
                tokio::fs::write(dir.join("mapper.toml"), "").await.unwrap();
            }

            let mapper_dir = config_dir.join("mappers/nonexistent");
            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("thingsboard") || msg.contains("mycloud"),
                "Error should list available mappers: {msg}"
            );
        }

        /// Flows-only mapper with no mapper.toml still appears in available list.
        #[tokio::test]
        async fn error_lists_flows_only_mappers_without_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            // A flows-only mapper: just a directory, no mapper.toml
            tokio::fs::create_dir_all(config_dir.join("mappers/myflows"))
                .await
                .unwrap();

            let mapper_dir = config_dir.join("mappers/nonexistent");
            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("myflows"),
                "Error should list flows-only mapper with no mapper.toml: {msg}"
            );
        }

        /// No mappers exist → error still mentions missing directory (not a panic).
        #[tokio::test]
        async fn error_when_no_mappers_exist() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            tokio::fs::create_dir_all(config_dir.join("mappers"))
                .await
                .unwrap();

            let mapper_dir = config_dir.join("mappers/nonexistent");
            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention missing directory: {msg}"
            );
        }

        /// No bridge/ dir → FlowsOnly regardless of whether flows/ exists.
        #[tokio::test]
        async fn starts_in_flows_only_mode_even_if_flows_directory_does_not_exist() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/testmapper");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(mapper_dir.join("mapper.toml"), "url = \"host:8883\"\n")
                .await
                .unwrap();

            let startup = validate_and_load(&mapper_dir, config_dir).await.unwrap();
            assert!(
                matches!(startup, MapperStartup::FlowsOnly),
                "Expected FlowsOnly"
            );
        }

        /// flows/ present, no bridge/ → FlowsOnly.
        #[tokio::test]
        async fn flows_only_when_no_bridge_dir() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/myflows");
            tokio::fs::create_dir_all(mapper_dir.join("flows"))
                .await
                .unwrap();

            let startup = validate_and_load(&mapper_dir, config_dir).await.unwrap();
            assert!(
                matches!(startup, MapperStartup::FlowsOnly),
                "Expected FlowsOnly"
            );
        }

        /// bridge/ present but no mapper.toml → error mentioning both.
        #[tokio::test]
        async fn errors_when_bridge_without_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/testmapper");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();

            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("bridge") && msg.contains("mapper.toml"),
                "Error should mention bridge/ and mapper.toml: {msg}"
            );
        }

        /// bridge/ + mapper.toml, no flows/ → WithBridge.
        #[tokio::test]
        async fn with_bridge_only_when_no_flows_dir() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/testmapper");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com\"\n",
            )
            .await
            .unwrap();

            let startup = validate_and_load(&mapper_dir, config_dir).await.unwrap();
            assert!(
                matches!(startup, MapperStartup::WithBridge { .. }),
                "Expected WithBridge {{ }}"
            );
        }

        /// bridge/ + mapper.toml + flows/ → WithBridge.
        #[tokio::test]
        async fn with_bridge_and_flows_when_both_present() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/testmapper");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();
            tokio::fs::create_dir_all(mapper_dir.join("flows"))
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com\"\n",
            )
            .await
            .unwrap();

            let startup = validate_and_load(&mapper_dir, config_dir).await.unwrap();
            assert!(
                matches!(startup, MapperStartup::WithBridge { .. }),
                "Expected WithBridge {{ }}"
            );
        }

        /// bridge/ + malformed mapper.toml → parse error mentioning mapper.toml.
        #[tokio::test]
        async fn errors_when_bridge_mapper_toml_is_malformed() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/testmapper");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();
            tokio::fs::write(mapper_dir.join("mapper.toml"), "not valid toml [[[\n")
                .await
                .unwrap();

            let err = validate_and_load(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err:#}");
            assert!(
                msg.contains("mapper.toml"),
                "Error should mention mapper.toml: {msg}"
            );
        }
    }

    mod build_flows_actors {
        use super::*;
        use tedge_config::TEdgeConfig;
        use tedge_test_utils::fs::TempTedgeDir;

        /// flows/ directory is created automatically when it does not exist.
        #[tokio::test]
        async fn creates_flows_directory_if_absent() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");

            build_flows_actors(&mapper_dir, "tedge-mapper-testmapper", &tedge_config)
                .await
                .unwrap();

            assert!(
                mapper_dir.join("flows").is_dir(),
                "flows/ directory should have been created"
            );
        }
    }

    mod cloud_mqtt_options {
        use super::*;
        use crate::custom::config::AuthMethodConfig;
        use crate::custom::config::BridgeConfig;
        use crate::custom::config::DeviceConfig;
        use crate::custom::resolve::resolve_effective_config;
        use tedge_config::TEdgeConfig;
        use tedge_test_utils::fs::TempTedgeDir;

        // Test-only EC certificate and matching private key (self-signed, CN=localhost).
        // Generated by crates/common/axum_tls/test_data/_regenerate_certs.sh.
        const TEST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIIBnzCCAUWgAwIBAgIUSTUtJUfUdERMKBwsfdRv9IbvQicwCgYIKoZIzj0EAwIw\n\
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTIzMTExNDE2MDUwOVoYDzMwMjMwMzE3\n\
MTYwNTA5WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO\n\
PQMBBwNCAAR2SVEPD34AAxFuk0xYm60p7hA7+1SW+sFHazBRg32ifFd0o2Mn+Tf+\n\
voYflBi3v4lhr361RoWB8QfmaGN05vv+o3MwcTAdBgNVHQ4EFgQUAb4jQ7RQ/xyg\n\
cZM+We8ik29/oxswHwYDVR0jBBgwFoAUAb4jQ7RQ/xygcZM+We8ik29/oxswIQYD\n\
VR0RBBowGIIJbG9jYWxob3N0ggsqLmxvY2FsaG9zdDAMBgNVHRMBAf8EAjAAMAoG\n\
CCqGSM49BAMCA0gAMEUCIA6QrxoDHQJqoly7d8VN0sj0eDvfFpbbZdSnzBd6R8AP\n\
AiEAm/PAH3IPGuHRBIpdC0rNR8F/l3WcN9I9984qKZdG5rs=\n\
-----END CERTIFICATE-----\n";

        const TEST_KEY_PEM: &str = "\
-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIBX2Z/NKGEX14QbH4kb5GXom0pqSPfX0mxdWbLb86apEoAoGCCqGSM49\n\
AwEHoUQDQgAEdklRDw9+AAMRbpNMWJutKe4QO/tUlvrBR2swUYN9onxXdKNjJ/k3\n\
/r6GH5QYt7+JYa9+tUaFgfEH5mhjdOb7/g==\n\
-----END EC PRIVATE KEY-----\n";

        async fn resolve(
            config: &CustomMapperConfig,
            tedge_config: &TEdgeConfig,
        ) -> EffectiveMapperConfig {
            resolve_effective_config(config, tedge_config, None, None)
                .await
                .unwrap()
        }

        #[tokio::test]
        async fn missing_url_returns_error_with_file_path_hint() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);
            let effective = resolve(&config, &tedge_config).await;

            let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("mapper.toml"),
                "Error should mention mapper.toml: {msg}"
            );
            assert!(msg.contains("url"), "Error should mention url: {msg}");
        }

        #[tokio::test]
        async fn auto_without_credentials_resolves_to_certificate() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:8883"));
            let effective = resolve(&config, &tedge_config).await;

            let (_, auth) = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
            assert!(matches!(auth, AuthMethod::Certificate));
        }

        #[tokio::test]
        async fn explicit_certificate_method_resolves_to_certificate() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let mut config = make_config(Some("mqtt.example.com:8883"));
            config.auth_method = AuthMethodConfig::Certificate;
            let effective = resolve(&config, &tedge_config).await;

            let (_, auth) = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
            assert!(matches!(auth, AuthMethod::Certificate));
        }

        #[tokio::test]
        async fn password_method_without_credentials_path_errors() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.auth_method = AuthMethodConfig::Password;
            let effective = resolve(&config, &tedge_config).await;

            let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("credentials_path"),
                "Error should mention credentials_path: {msg}"
            );
        }

        #[tokio::test]
        async fn auto_with_credentials_path_resolves_to_password() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let creds_path = ttd.utf8_path().join("creds.toml");
            tokio::fs::write(
                &creds_path,
                "[credentials]\nusername = \"alice\"\npassword = \"secret\"\n",
            )
            .await
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("device.id = \"test-device\"");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.credentials_path = Some(creds_path);
            let effective = resolve(&config, &tedge_config).await;

            let (_, auth) = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
            assert!(matches!(auth, AuthMethod::Password));
        }

        #[tokio::test]
        async fn explicit_password_method_with_credentials_path_resolves_to_password() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
            let creds_path = ttd.utf8_path().join("creds.toml");
            tokio::fs::write(
                &creds_path,
                "[credentials]\nusername = \"bob\"\npassword = \"pass\"\n",
            )
            .await
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("device.id = \"test-device\"");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.auth_method = AuthMethodConfig::Password;
            config.credentials_path = Some(creds_path);
            let effective = resolve(&config, &tedge_config).await;

            let (_, auth) = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
            assert!(matches!(auth, AuthMethod::Password));
        }

        #[tokio::test]
        async fn mapper_toml_cert_takes_precedence_over_tedge_config() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let tedge_config = TEdgeConfig::load_toml_str(
                "device.cert_path = \"/nonexistent/tedge-cert.pem\"\n\
                 device.key_path = \"/nonexistent/tedge-key.pem\"\n",
            );
            let (mapper_cert, mapper_key) = write_cert(ttd.utf8_path()).await;
            let mut config = make_config(Some("mqtt.example.com:8883"));
            config.device = Some(DeviceConfig {
                id: None,
                cert_path: Some(mapper_cert),
                key_path: Some(mapper_key),
                root_cert_path: None,
            });
            let effective = resolve(&config, &tedge_config).await;

            build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
        }

        #[tokio::test]
        async fn absent_mapper_cert_falls_back_to_tedge_config() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:8883"));
            let effective = resolve(&config, &tedge_config).await;

            build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap();
        }

        #[tokio::test]
        async fn cert_cn_used_as_mqtt_client_id() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let (cert, key) = write_cert(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:8883"));
            let effective = resolve(&config, &tedge_config).await;

            let (mqtt_opts, _) =
                build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                    .await
                    .unwrap();
            assert_eq!(mqtt_opts.client_id(), "localhost");
        }

        #[tokio::test]
        async fn password_auth_uses_device_id_as_client_id() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let creds_path = ttd.utf8_path().join("creds.toml");
            write_creds(&creds_path).await;
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.credentials_path = Some(creds_path);
            config.device = Some(DeviceConfig {
                id: Some("mapper-device".to_string()),
                cert_path: None,
                key_path: None,
                root_cert_path: None,
            });
            let effective = resolve(&config, &tedge_config).await;

            let (mqtt_opts, _) =
                build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                    .await
                    .unwrap();
            assert_eq!(mqtt_opts.client_id(), "mapper-device");
        }

        #[tokio::test]
        async fn password_auth_without_device_id_returns_error() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let creds_path = ttd.utf8_path().join("creds.toml");
            write_creds(&creds_path).await;
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.credentials_path = Some(creds_path);
            let effective = resolve(&config, &tedge_config).await;

            let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("device.id"),
                "Error should mention device.id: {msg}"
            );
        }

        #[tokio::test]
        async fn cert_auth_without_device_id_returns_error() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            let (cert, key) = write_cert_no_cn(ttd.utf8_path()).await;
            let tedge_config = TEdgeConfig::load_toml_str(&format!(
                "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
            ));
            let config = make_config(Some("mqtt.example.com:8883"));
            let effective = resolve(&config, &tedge_config).await;

            let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("device.id"),
                "Error should mention device.id: {msg}"
            );
        }

        fn make_config(url: Option<&str>) -> CustomMapperConfig {
            CustomMapperConfig {
                table: toml::Table::new(),
                cloud_type: None,
                url: url.map(|u| u.parse().unwrap()),
                device: None,
                bridge: BridgeConfig::default(),
                auth_method: AuthMethodConfig::Auto,
                credentials_path: None,
            }
        }

        async fn write_cert(dir: &camino::Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
            let cert = dir.join("cert.pem");
            let key = dir.join("key.pem");
            tokio::fs::write(&cert, TEST_CERT_PEM).await.unwrap();
            tokio::fs::write(&key, TEST_KEY_PEM).await.unwrap();
            (cert, key)
        }

        async fn write_cert_no_cn(dir: &camino::Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
            let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
            let mut params = rcgen::CertificateParams::default();
            params.distinguished_name = rcgen::DistinguishedName::new();
            let issuer = rcgen::Issuer::from_params(&params, &key);
            let cert = params.signed_by(&key, &issuer).unwrap();
            let cert_path = dir.join("cert-no-cn.pem");
            let key_path = dir.join("key-no-cn.pem");
            tokio::fs::write(&cert_path, cert.pem()).await.unwrap();
            tokio::fs::write(&key_path, key.serialize_pem())
                .await
                .unwrap();
            (cert_path, key_path)
        }

        async fn write_creds(path: &Utf8PathBuf) {
            tokio::fs::write(
                path,
                "[credentials]\nusername = \"user\"\npassword = \"pass\"\n",
            )
            .await
            .unwrap();
        }

        mod tls {
            use super::*;

            #[tokio::test]
            async fn tls_off_with_password_auth_connects_without_tls() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let creds_path = ttd.utf8_path().join("creds.toml");
                write_creds(&creds_path).await;
                let tedge_config = TEdgeConfig::load_toml_str("device.id = \"test-device\"");
                let mut config = make_config(Some("mqtt.example.com:1883"));
                config.bridge.tls.enable = BridgeTlsEnable::False;
                config.credentials_path = Some(creds_path);
                let effective = resolve(&config, &tedge_config).await;

                let (config, _) =
                    build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                        .await
                        .unwrap();

                assert!(
                    matches!(
                        config.transport(),
                        tedge_mqtt_bridge::rumqttc::Transport::Tcp
                    ),
                    "Transport should be plain TCP when TLS is disabled",
                );
            }

            #[tokio::test]
            async fn tls_on_with_cert_auth_connects_with_tls() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let (cert, key) = write_cert(ttd.utf8_path()).await;
                let tedge_config = TEdgeConfig::load_toml_str(&format!(
                    "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
                ));
                let mut config = make_config(Some("mqtt.example.com:8883"));
                config.bridge.tls.enable = BridgeTlsEnable::True;
                let effective = resolve(&config, &tedge_config).await;

                let (config, _) =
                    build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                        .await
                        .unwrap();

                assert!(
                    matches!(
                        config.transport(),
                        tedge_mqtt_bridge::rumqttc::Transport::Tls(..)
                    ),
                    "Transport should be TLS",
                );
            }

            #[tokio::test]
            async fn auto_port_8883_infers_tls_on() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let (cert, key) = write_cert(ttd.utf8_path()).await;
                let tedge_config = TEdgeConfig::load_toml_str(&format!(
                    "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
                ));
                let config = make_config(Some("mqtt.example.com:8883"));
                assert_eq!(config.bridge.tls.enable, BridgeTlsEnable::Auto);
                let effective = resolve(&config, &tedge_config).await;

                let (config, _) =
                    build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                        .await
                        .unwrap();

                assert!(
                    matches!(
                        config.transport(),
                        tedge_mqtt_bridge::rumqttc::Transport::Tls(..)
                    ),
                    "Transport should be TLS",
                );
            }

            #[tokio::test]
            async fn auto_port_1883_infers_tls_off() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let creds_path = ttd.utf8_path().join("creds.toml");
                write_creds(&creds_path).await;
                let tedge_config = TEdgeConfig::load_toml_str("device.id = \"test-device\"");
                let mut config = make_config(Some("mqtt.example.com:1883"));
                config.credentials_path = Some(creds_path);
                assert_eq!(config.bridge.tls.enable, BridgeTlsEnable::Auto);
                let effective = resolve(&config, &tedge_config).await;

                let (config, _) =
                    build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                        .await
                        .unwrap();

                assert!(
                    matches!(
                        config.transport(),
                        tedge_mqtt_bridge::rumqttc::Transport::Tcp
                    ),
                    "Transport should be plain TCP",
                );
            }

            #[tokio::test]
            async fn other_port_defaults_to_tls_on() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let (cert, key) = write_cert(ttd.utf8_path()).await;
                let tedge_config = TEdgeConfig::load_toml_str(&format!(
                    "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
                ));
                let config = make_config(Some("mqtt.example.com:9999"));
                assert_eq!(config.bridge.tls.enable, BridgeTlsEnable::Auto);
                let effective = resolve(&config, &tedge_config).await;

                let (config, _) =
                    build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                        .await
                        .unwrap();

                assert!(
                    matches!(
                        config.transport(),
                        tedge_mqtt_bridge::rumqttc::Transport::Tls(..)
                    ),
                    "Transport should be TLS",
                );
            }

            #[tokio::test]
            async fn tls_off_with_cert_auth_fails() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let (cert, key) = write_cert(ttd.utf8_path()).await;
                let tedge_config = TEdgeConfig::load_toml_str(&format!(
                    "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
                ));
                let mut config = make_config(Some("mqtt.example.com:8883"));
                config.bridge.tls.enable = BridgeTlsEnable::False;
                config.auth_method = AuthMethodConfig::Certificate;
                let effective = resolve(&config, &tedge_config).await;

                let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                    .await
                    .unwrap_err();
                assert_eq!(
                    err.to_string(),
                    "certificate authentication requires TLS, but TLS is disabled for this mapper. \
                     Either set `[bridge]\ntls = \"on\"` in mapper.toml, or change the auth method."
                );
            }

            #[tokio::test]
            async fn auto_tls_inferred_off_with_cert_auth_fails() {
                let ttd = TempTedgeDir::new();
                let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
                let (cert, key) = write_cert(ttd.utf8_path()).await;
                let tedge_config = TEdgeConfig::load_toml_str(&format!(
                    "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
                ));
                let mut config = make_config(Some("mqtt.example.com:1883"));
                config.auth_method = AuthMethodConfig::Certificate;
                let effective = resolve(&config, &tedge_config).await;

                let err = build_cloud_mqtt_options(&effective, "svc", &mapper_dir, &tedge_config)
                    .await
                    .unwrap_err();
                assert_eq!(
                    err.to_string(),
                    "certificate authentication requires TLS, but TLS is disabled for this mapper. \
                     Either set `[bridge]\ntls = \"on\"` in mapper.toml, or change the auth method."
                );
            }
        }
    }
}
