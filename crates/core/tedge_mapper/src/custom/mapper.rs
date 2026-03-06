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
use tedge_config::tedge_toml::ProfileName;
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
        let formatted: Vec<String> = available
            .into_iter()
            .map(|p| match p {
                None => "(default, no --profile)".to_string(),
                Some(name) => format!("--profile {name}"),
            })
            .collect();
        anyhow::bail!(
            "Custom mapper directory '{}' does not exist. \
             Available custom mapper profiles: {}",
            mapper_dir,
            formatted.join(", ")
        );
    }
}

/// Lists available custom mapper profiles by scanning the mappers directory.
///
/// Returns `None` for the default (`custom/`) directory and `Some(name)` for each
/// `custom.{name}/` directory found.
fn list_custom_profiles(mappers_dir: &Utf8Path) -> Vec<Option<String>> {
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
            profiles.push(None);
        } else if let Some(profile) = name.strip_prefix("custom.") {
            profiles.push(Some(profile.to_string()));
        }
    }
    profiles.sort();
    profiles
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
        format!("'{mapper_dir}/tedge.toml' is missing a 'url' field required for the MQTT bridge",)
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
                    "'{mapper_dir}/tedge.toml' sets auth_method = \"password\" but \
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
/// A directory containing only `tedge.toml` (and no `bridge/` or `flows/`
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

/// Validates the mapper directory's startup config, returning the parsed config if `tedge.toml`
/// is present, `None` if only flows are present, or an error if `bridge/` exists without a
/// `tedge.toml`.
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
                 'tedge.toml' connection config. \
                 Create a tedge.toml with a top-level 'url' field (e.g. url = \"host:8883\") to use the MQTT bridge.",
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

        validate_profile_dir(&mapper_dir, config_dir)?;
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
                msg.contains("bridge") && msg.contains("tedge.toml"),
                "Error should mention bridge/ and tedge.toml: {msg}"
            );
        }

        #[tokio::test]
        async fn bridge_dir_with_tedge_toml_returns_config() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            tokio::fs::create_dir_all(mapper_dir.join("bridge"))
                .await
                .unwrap();
            tokio::fs::write(
                mapper_dir.join("tedge.toml"),
                "url = \"mqtt.example.com\"\n",
            )
            .await
            .unwrap();

            let config = check_startup_config(&mapper_dir).await.unwrap();
            assert!(
                config.is_some(),
                "Should return config when tedge.toml exists"
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

        /// A directory with only tedge.toml (no bridge/ or flows/) has no active components
        #[test]
        fn tedge_toml_only_errors_no_active_components() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
            std::fs::create_dir_all(&mapper_dir).unwrap();
            std::fs::write(mapper_dir.join("tedge.toml"), "url = \"host:8883\"\n").unwrap();

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
                msg.contains("tedge.toml"),
                "Error should mention tedge.toml: {msg}"
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
            }
        }
    }
}
