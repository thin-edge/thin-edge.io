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
use crate::custom::config::AuthMethodConfig;
use anyhow::Context;
use async_trait::async_trait;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config::tedge_toml::MqttAuthClientConfigCloudBroker;
use tedge_config::tedge_toml::MqttAuthConfigCloudBroker;
use tedge_config::tedge_toml::PrivateKeyType;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::ConnectedFlowRegistry;
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

    /// Returns the systemd service name: `tedge-mapper@{name}`.
    pub fn service_name(&self) -> String {
        format!("tedge-mapper@{}", self.name)
    }

    /// Returns the bridge service name: `tedge-mapper-bridge-{name}`.
    pub fn bridge_service_name(&self) -> String {
        format!("tedge-mapper-bridge-{}", self.name)
    }
}

/// Validates that the mapper directory exists and contains a `mapper.toml` file.
///
/// If the directory is missing or has no `mapper.toml`, returns an error listing
/// available mappers found in the mappers directory.
pub async fn validate_mapper_dir(
    mapper_dir: &Utf8Path,
    config_dir: &Utf8Path,
) -> anyhow::Result<()> {
    let mappers_root = config_dir.join("mappers");

    if !tokio::fs::try_exists(mapper_dir).await.unwrap_or(false)
        || !tokio::fs::metadata(mapper_dir)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    {
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

    anyhow::ensure!(
        tokio::fs::try_exists(mapper_dir.join("mapper.toml"))
            .await
            .unwrap_or(false),
        "Mapper directory '{mapper_dir}' does not contain a 'mapper.toml' file. \
         Create a mapper.toml to configure this mapper."
    );

    Ok(())
}

/// Lists available mappers by scanning the mappers root directory for subdirectories
/// that contain a `mapper.toml` file.
async fn list_available_mappers(mappers_root: &Utf8Path) -> Vec<String> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_root).await else {
        return Vec::new();
    };

    let mut names = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(file_type) = entry.file_type().await else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        if tokio::fs::try_exists(path.join("mapper.toml"))
            .await
            .unwrap_or(false)
        {
            if let Some(name) = path.file_name() {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// Builds the [`MqttOptions`] for the cloud broker connection described by `config`.
///
/// Reads `config.url`, sets clean-session and keepalive, resolves the CA path,
/// determines the effective auth method, and configures either certificate TLS or
/// password credentials. Also applies any HTTP proxy settings from `tedge_config`.
///
/// Returns `(MqttOptions, AuthMethod)` so the caller can pass `AuthMethod` to
/// `load_bridge_rules_from_directory`.
pub fn build_cloud_mqtt_options(
    config: &crate::custom::config::CustomMapperConfig,
    service_name: &str,
    mapper_dir: &Utf8Path,
    tedge_config: &TEdgeConfig,
) -> anyhow::Result<(MqttOptions, AuthMethod)> {
    let url = config.url.as_ref().with_context(|| {
        format!("'{mapper_dir}/mapper.toml' is missing a 'url' field required for the MQTT bridge",)
    })?;

    let mut cloud_config = MqttOptions::new(service_name, url.host().to_string(), url.port().0);
    cloud_config.set_clean_session(config.bridge.clean_session);
    if let Some(interval) = &config.bridge.keepalive_interval {
        cloud_config.set_keep_alive(interval.duration());
    }

    let ca_path = config
        .device
        .as_ref()
        .and_then(|d| d.root_cert_path.clone())
        .unwrap_or_else(|| Utf8PathBuf::from("/etc/ssl/certs"));

    // Resolve the effective auth method
    let has_credentials = config.credentials_path.is_some();
    let effective_auth = match config.auth_method {
        AuthMethodConfig::Certificate => AuthMethod::Certificate,
        AuthMethodConfig::Password => AuthMethod::Password,
        AuthMethodConfig::Auto => {
            if has_credentials {
                AuthMethod::Password
            } else {
                AuthMethod::Certificate
            }
        }
    };

    match effective_auth {
        AuthMethod::Certificate => {
            if let Some(device) = &config.device {
                if let (Some(cert_path), Some(key_path)) = (&device.cert_path, &device.key_path) {
                    let tls_config = MqttAuthConfigCloudBroker {
                        ca_path,
                        client: Some(MqttAuthClientConfigCloudBroker {
                            cert_file: cert_path.clone(),
                            private_key: PrivateKeyType::File(key_path.clone()),
                        }),
                    }
                    .to_rustls_client_config()
                    .context("Failed to create MQTT TLS config")?;
                    cloud_config.set_transport(Transport::tls_with_config(tls_config.into()));

                    // device.id takes precedence over cert CN
                    let client_id = device.id.clone().or_else(|| {
                        PemCertificate::from_pem_file(cert_path)
                            .and_then(|cert| cert.subject_common_name())
                            .ok()
                            .filter(|cn| !cn.is_empty())
                    });
                    if let Some(id) = client_id {
                        cloud_config.set_client_id(id);
                    }
                }
            }
        }
        AuthMethod::Password => {
            let creds_path = config.credentials_path.as_deref().with_context(|| {
                format!(
                    "'{mapper_dir}/mapper.toml' sets auth_method = \"password\" but \
                     no credentials_path is configured"
                )
            })?;
            let (username, password) = read_mapper_credentials(creds_path)?;
            use_credentials(&mut cloud_config, &ca_path, username, password)?;

            // Apply explicit device.id if set
            if let Some(id) = config.device.as_ref().and_then(|d| d.id.as_ref()) {
                cloud_config.set_client_id(id.clone());
            }
        }
    }

    configure_proxy(tedge_config, &mut cloud_config)?;

    Ok((cloud_config, effective_auth))
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
    let service_topic_id = EntityTopicId::default_main_service(service_name)?;
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
    let flows = ConnectedFlowRegistry::new(flows_dir);
    let fs_actor = FsWatchActorBuilder::new();
    let cmd_watcher_actor = WatchActorBuilder::new();
    let flows_mapper = FlowsMapperBuilder::try_new(flows, service_config).await?;

    Ok((flows_mapper, fs_actor, cmd_watcher_actor))
}

/// Validates that the mapper directory has at least one active component
/// (bridge or flows).
///
/// A directory containing only `mapper.toml` (and no `bridge/` or `flows/`
/// directories) would start with nothing to do.
pub fn check_has_active_components(mapper_dir: &Utf8Path) -> anyhow::Result<()> {
    let has_bridge_dir = mapper_dir.join("bridge").is_dir();
    let has_flows_dir = mapper_dir.join("flows").is_dir();
    if !has_bridge_dir && !has_flows_dir {
        anyhow::bail!(
            "Custom mapper directory '{}' contains neither a 'bridge/' nor a 'flows/' \
             directory — the mapper would do nothing. Add bridge rules, flow scripts, \
             or both.",
            mapper_dir
        );
    }
    Ok(())
}

/// Validates the mapper directory's startup config, returning the parsed config if `mapper.toml`
/// is present, `None` if only flows are present, or an error if `bridge/` exists without a
/// `mapper.toml`.
///
/// This is a separate function to make the startup validation testable without requiring a
/// live MQTT broker.
pub async fn check_startup_config(
    mapper_dir: &Utf8Path,
) -> anyhow::Result<Option<crate::custom::config::CustomMapperConfig>> {
    let has_bridge_dir = mapper_dir.join("bridge").is_dir();
    if !has_bridge_dir {
        return Ok(None);
    }
    let mapper_config = load_mapper_config(mapper_dir).await?;
    match mapper_config {
        None => {
            anyhow::bail!(
                "Mapper directory '{mapper_dir}' contains a 'bridge/' subdirectory but no \
                 'mapper.toml' connection config. \
                 Create a mapper.toml with a top-level 'url' field (e.g. url = \"host:8883\") to use the MQTT bridge.",
            );
        }
        Some(config) => Ok(Some(config)),
    }
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

        validate_mapper_dir(&mapper_dir, config_dir).await?;
        check_has_active_components(&mapper_dir)?;

        let has_flows_dir = mapper_dir.join("flows").is_dir();

        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(&service_name, &tedge_config).await?;

        if let Some(config) = check_startup_config(&mapper_dir).await? {
            let bridge_dir = mapper_dir.join("bridge");
            let bridge_service_name = self.bridge_service_name();
            let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
            let device_topic_id = tedge_config.mqtt.device_topic_id.clone();
            let health_topic =
                service_health_topic(&mqtt_schema, &device_topic_id, &bridge_service_name);

            let (cloud_config, effective_auth) =
                build_cloud_mqtt_options(&config, &service_name, &mapper_dir, &tedge_config)?;

            let bridge_rules = load_bridge_rules_from_directory(
                &bridge_dir,
                &tedge_config,
                effective_auth,
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

        if has_flows_dir {
            let (mut flows_mapper, mut fs_actor, mut cmd_watcher_actor) =
                build_flows_actors(&mapper_dir, &service_name, &tedge_config).await?;
            flows_mapper.connect(&mut mqtt_actor);
            flows_mapper.connect_fs(&mut fs_actor);
            flows_mapper.connect_cmd(&mut cmd_watcher_actor);

            runtime.spawn(flows_mapper).await?;
            runtime.spawn(fs_actor).await?;
            runtime.spawn(cmd_watcher_actor).await?;
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
        fn uses_mapper_name_in_service_names() {
            let mapper = CustomMapper {
                name: "thingsboard".to_string(),
            };
            assert_eq!(mapper.service_name(), "tedge-mapper@thingsboard");
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

    mod mapper_validation {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn succeeds_when_mapper_toml_exists() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(mapper_dir.join("mapper.toml"), "")
                .await
                .unwrap();

            assert!(validate_mapper_dir(&mapper_dir, config_dir).await.is_ok());
        }

        #[tokio::test]
        async fn errors_when_directory_missing() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/nonexistent");

            let err = validate_mapper_dir(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention directory missing: {msg}"
            );
        }

        #[tokio::test]
        async fn errors_when_no_mapper_toml() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            let mapper_dir = config_dir.join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            // No mapper.toml

            let err = validate_mapper_dir(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("mapper.toml"),
                "Error should mention missing mapper.toml: {msg}"
            );
        }

        #[tokio::test]
        async fn error_lists_available_mappers() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();

            let thingsboard_dir = config_dir.join("mappers/thingsboard");
            tokio::fs::create_dir_all(&thingsboard_dir).await.unwrap();
            tokio::fs::write(thingsboard_dir.join("mapper.toml"), "")
                .await
                .unwrap();
            let mycloud_dir = config_dir.join("mappers/mycloud");
            tokio::fs::create_dir_all(&mycloud_dir).await.unwrap();
            tokio::fs::write(mycloud_dir.join("mapper.toml"), "")
                .await
                .unwrap();

            let mapper_dir = config_dir.join("mappers/nonexistent");
            let err = validate_mapper_dir(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");

            assert!(
                msg.contains("thingsboard") || msg.contains("mycloud"),
                "Error should list available mappers: {msg}"
            );
        }

        #[tokio::test]
        async fn error_when_no_mappers_exist() {
            let ttd = TempTedgeDir::new();
            let config_dir = ttd.utf8_path();
            tokio::fs::create_dir_all(config_dir.join("mappers"))
                .await
                .unwrap();

            let mapper_dir = config_dir.join("mappers/nonexistent");
            let err = validate_mapper_dir(&mapper_dir, config_dir)
                .await
                .unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("does not exist"),
                "Error should mention missing directory: {msg}"
            );
        }
    }

    mod startup_config {
        use super::*;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn bridge_dir_without_tedge_toml_errors() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();

            let err = check_startup_config(&mapper_dir).await.unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("bridge") && msg.contains("mapper.toml"),
                "Error should mention bridge/ and mapper.toml: {msg}"
            );
        }

        #[tokio::test]
        async fn bridge_dir_with_mapper_toml_returns_config() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "url = \"mqtt.example.com\"\n",
            )
            .await
            .unwrap();

            let config = check_startup_config(&mapper_dir).await.unwrap();
            assert!(
                config.is_some(),
                "Should return config when mapper.toml exists"
            );
        }

        /// No bridge/ directory returns None (flows-only case)
        #[tokio::test]
        async fn no_bridge_dir_returns_none() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.flows-only");
            tokio::fs::create_dir_all(mapper_dir.join("flows"))
                .await
                .unwrap();

            let config = check_startup_config(&mapper_dir).await.unwrap();
            assert!(
                config.is_none(),
                "Should return None when no bridge/ dir exists"
            );
        }

        /// A directory with only mapper.toml (no bridge/ or flows/) has no active components
        #[test]
        fn mapper_toml_only_errors_no_active_components() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("mapper.toml"), "url = \"host:8883\"\n").unwrap();

            let err = check_has_active_components(&mapper_dir).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("bridge") && msg.contains("flows"),
                "Error should mention both bridge/ and flows/: {msg}"
            );
        }

        /// A directory with bridge/ passes the active-components check
        #[test]
        fn bridge_dir_passes_active_components_check() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            std::fs::create_dir_all(mapper_dir.join("bridge")).unwrap();

            assert!(check_has_active_components(&mapper_dir).is_ok());
        }

        /// A directory with flows/ passes the active-components check
        #[test]
        fn flows_dir_passes_active_components_check() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            std::fs::create_dir_all(mapper_dir.join("flows")).unwrap();

            assert!(check_has_active_components(&mapper_dir).is_ok());
        }
    }

    mod cloud_mqtt_options {
        use super::*;
        use crate::custom::config::AuthMethodConfig;
        use crate::custom::config::BridgeConfig;
        use crate::custom::config::CustomMapperConfig;
        use tedge_config::TEdgeConfig;
        use tedge_test_utils::fs::TempTedgeDir;

        #[test]
        fn missing_url_returns_error_with_file_path_hint() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(None);

            let err =
                build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("mapper.toml"),
                "Error should mention mapper.toml: {msg}"
            );
            assert!(msg.contains("url"), "Error should mention url: {msg}");
        }

        #[test]
        fn auto_without_credentials_resolves_to_certificate() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let config = make_config(Some("mqtt.example.com:1883"));

            let (_, auth) =
                build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config).unwrap();
            assert!(matches!(auth, AuthMethod::Certificate));
        }

        #[test]
        fn explicit_certificate_method_resolves_to_certificate() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.auth_method = AuthMethodConfig::Certificate;

            let (_, auth) =
                build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config).unwrap();
            assert!(matches!(auth, AuthMethod::Certificate));
        }

        #[test]
        fn password_method_without_credentials_path_errors() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.auth_method = AuthMethodConfig::Password;

            let err =
                build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config).unwrap_err();
            let msg = format!("{err}");
            assert!(
                msg.contains("credentials_path"),
                "Error should mention credentials_path: {msg}"
            );
        }

        #[test]
        fn auto_with_credentials_path_resolves_to_password() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let creds_path = ttd.utf8_path().join("creds.toml");
            std::fs::write(
                &creds_path,
                "[credentials]\nusername = \"alice\"\npassword = \"secret\"\n",
            )
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.credentials_path = Some(creds_path);

            // auth_method = Auto + credentials_path set → resolves to Password
            match build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config) {
                Ok((_, auth)) => assert!(matches!(auth, AuthMethod::Password)),
                Err(e) => {
                    // Only acceptable failure is a TLS/CA setup issue (system-dependent);
                    // the auth resolution itself must have succeeded.
                    let msg = format!("{e}");
                    assert!(
                        !msg.contains("credentials_path"),
                        "Auth resolution should succeed (credentials_path is set): {msg}"
                    );
                }
            }
        }

        #[test]
        fn explicit_password_method_with_credentials_path_resolves_to_password() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            let creds_path = ttd.utf8_path().join("creds.toml");
            std::fs::write(
                &creds_path,
                "[credentials]\nusername = \"bob\"\npassword = \"pass\"\n",
            )
            .unwrap();
            let tedge_config = TEdgeConfig::load_toml_str("");
            let mut config = make_config(Some("mqtt.example.com:1883"));
            config.auth_method = AuthMethodConfig::Password;
            config.credentials_path = Some(creds_path);

            match build_cloud_mqtt_options(&config, "svc", &mapper_dir, &tedge_config) {
                Ok((_, auth)) => assert!(matches!(auth, AuthMethod::Password)),
                Err(e) => {
                    let msg = format!("{e}");
                    assert!(
                        !msg.contains("credentials_path"),
                        "Auth resolution should succeed (credentials_path is set): {msg}"
                    );
                }
            }
        }

        fn make_config(url: Option<&str>) -> CustomMapperConfig {
            CustomMapperConfig {
                table: toml::Table::new(),
                url: url.map(|u| u.parse().unwrap()),
                device: None,
                bridge: BridgeConfig::default(),
                auth_method: AuthMethodConfig::Auto,
                credentials_path: None,
                cloud_type: None,
            }
        }
    }
}
