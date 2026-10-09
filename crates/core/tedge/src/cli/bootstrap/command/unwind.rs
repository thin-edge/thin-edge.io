//! The two deliberate backward transitions:
//! `--re-register` drops the registration artifacts (the outputs),
//! `--clean` also unwinds the instance's own configuration (the inputs).
//!
//! Bootstrap unwinds only what it writes: the rest of the cloud's config
//! section, device-global keys, and package-shipped mapper content
//! are never removed (removing `mappers/<name>/` outright belongs to a
//! future `tedge mapper remove`).

use super::BootstrapCommand;
use crate::cli::bootstrap::mapper_toml::instance_cert_dir;
use crate::cli::bootstrap::mapper_toml::MapperToml;
use crate::cli::bootstrap::settings::key;
use crate::cli::common::custom_mapper_service_name;
use crate::cli::common::is_builtin_cloud;
use crate::cli::common::Cloud;
use crate::log::MaybeFancy;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use strum::IntoEnumIterator;
use tedge_config::tedge_toml::models::CloudType;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_mapper::custom_mapper_config::scan_mappers_shallow;
use tedge_system_services::SystemService;

/// The instance-scoped keys bootstrap writes for a built-in cloud,
/// relative to the instance (`c8y.` or `c8y.profiles.<p>.`):
/// what `--clean` unwinds, leaving the rest of the cloud's section alone
const BUILTIN_MANAGED_KEYS: &[&str] = &[
    key::URL,
    key::HTTP,
    key::MQTT,
    key::AUTH_METHOD,
    key::CREDENTIALS_PATH,
    key::BRIDGE_TOPIC_PREFIX,
    key::PROXY_BIND_PORT,
    key::DEVICE_CERT_PATH,
    key::DEVICE_CSR_PATH,
];

/// The keys bootstrap writes into a custom mapper's `mapper.toml`
const MAPPER_MANAGED_KEYS: &[&str] = &[
    key::URL,
    key::DEVICE_ID,
    key::AUTH_METHOD,
    key::CREDENTIALS_PATH,
    key::CLOUD_TYPE,
    key::BRIDGE_TOPIC_PREFIX,
    key::PROXY_BIND_PORT,
    key::DEVICE_CERT_PATH,
    key::DEVICE_KEY_PATH,
];

impl BootstrapCommand {
    /// Remove the instance's registration artifacts (stopping its mapper),
    /// so the registration step runs afresh instead of skipping.
    ///
    /// Instance-scoped artifacts are removed unconditionally;
    /// the default instance's certificate is the shared device certificate,
    /// removed with a warning since other cloud connections may use it.
    pub(super) async fn remove_registration_artifacts(
        &self,
        config: &TEdgeConfig,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.custom_mapper_name() {
            Some(name) => {
                let service_name = custom_mapper_service_name(name);
                if self.dry_run {
                    self.detail(&format!("would stop and disable {service_name}"));
                } else {
                    let service = SystemService::new(&service_name);
                    // Best-effort: the service may not be installed or running
                    let _ = self.service_manager.stop_service(service).await;
                    let _ = self.service_manager.disable_service(service).await;
                }
                let mapper_toml = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
                self.remove(&mapper_toml.credentials_path()).await?;
                self.remove(&instance_cert_dir(&self.mapper_dir(name)))
                    .await?;
            }
            None => {
                let cert_path: Utf8PathBuf = config
                    .device_cert_path(Some(&self.cloud))
                    .map_err(anyhow::Error::new)?
                    .into();
                let global_cert: Utf8PathBuf = config
                    .device_cert_path(None::<&Cloud>)
                    .map_err(anyhow::Error::new)?
                    .into();
                let shared = cert_path == global_cert;
                if shared && cert_path.exists() {
                    self.warn_shared_certificate(config, &cert_path).await;
                }
                self.remove(&cert_path).await?;
                if let Some(csr) = self.read_instance_setting(config, key::DEVICE_CSR_PATH) {
                    self.remove(Utf8Path::new(&csr)).await?;
                }
                if self.is_c8y() {
                    let c8y_config = self.c8y_config(config)?;
                    let credentials: Utf8PathBuf =
                        c8y_config.cloud_specific.credentials_path.clone().into();
                    self.remove(&credentials).await?;
                }
                // The private key can be recreated for the default instance,
                // but is shared *by* named instances and profiles —
                // remove it only together with the shared certificate
                if shared {
                    let key_path: Utf8PathBuf = config
                        .device_key_path(Some(&self.cloud))
                        .map_err(anyhow::Error::new)?
                        .into();
                    self.remove(&key_path).await?;
                }
            }
        }
        Ok(())
    }

    /// Unwind the instance's own configuration (`--clean`):
    /// the keys bootstrap manages in the cloud's tedge config section,
    /// or in a custom mapper's mapper.toml — plus the settings this run
    /// applies (descriptor-implied and --set values scoped to the instance).
    ///
    /// Returns a configuration snapshot reflecting the unwind.
    pub(super) async fn unwind_instance_config(
        &self,
        config: TEdgeConfig,
    ) -> anyhow::Result<TEdgeConfig> {
        let managed = |managed: &[&str]| -> Vec<String> {
            managed
                .iter()
                .map(|key| (*key).to_owned())
                .chain(self.method_settings.iter().map(|s| s.key.clone()))
                .collect()
        };
        match self.custom_mapper_name() {
            Some(name) => {
                let path = self.mapper_config_path(name);
                if self.dry_run {
                    self.detail(&format!("would unset the bootstrap-managed keys in {path}"));
                    return Ok(config);
                }
                if path.exists() {
                    let mut mapper_toml = MapperToml::load(&path).await?;
                    // --set values are full keys: only this mapper's own are unwound
                    let own_prefix = format!("{name}.");
                    let own_settings = self
                        .settings
                        .iter()
                        .filter_map(|s| s.key.strip_prefix(&own_prefix))
                        .map(str::to_owned);
                    for key in managed(MAPPER_MANAGED_KEYS).into_iter().chain(own_settings) {
                        mapper_toml.unset(&key);
                    }
                    mapper_toml.save().await?;
                    self.detail(&format!("unset the bootstrap-managed keys in {path}"));
                }
                Ok(config)
            }
            None => {
                // c8y.* for the default instance, c8y.profiles.<p>.* for a
                // profile — never another instance's keys
                let prefix = self.instance_key("");
                let profiles_prefix = format!("{}.profiles.", self.cloud_name());
                let own_settings = self
                    .settings
                    .iter()
                    .map(|s| s.key.clone())
                    .filter(|key| key.starts_with(&prefix))
                    .filter(|key| self.profile().is_some() || !key.starts_with(&profiles_prefix));
                let keys: Vec<WritableKey> = managed(BUILTIN_MANAGED_KEYS)
                    .into_iter()
                    .map(|key| format!("{prefix}{key}"))
                    .chain(own_settings)
                    .filter_map(|key| key.parse::<WritableKey>().ok())
                    .collect();
                if self.dry_run {
                    self.detail(&format!("would unset the bootstrap-managed {prefix}* keys"));
                    return Ok(config);
                }
                config
                    .update_toml(&|dto, _reader| {
                        for key in &keys {
                            dto.try_unset_key(key)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(anyhow::Error::new)?;
                self.detail(&format!("unset the bootstrap-managed {prefix}* keys"));
                self.load_config().await
            }
        }
    }

    /// The shared certificate is about to be removed: name who else uses it
    async fn warn_shared_certificate(&self, config: &TEdgeConfig, cert_path: &Utf8Path) {
        let mut others: Vec<String> = CloudType::iter()
            .map(|cloud| cloud.as_ref().to_owned())
            .filter(|cloud| cloud != self.cloud_name())
            .filter(|cloud| {
                super::read_config_string(config, &format!("{cloud}.{}", key::URL)).is_some()
            })
            .collect();
        let custom_mappers = scan_mappers_shallow(&self.config_dir.join("mappers"))
            .await
            .into_iter()
            .filter(|(_, mapper_toml)| mapper_toml.is_some())
            .map(|(name, _)| name)
            .filter(|name| !is_builtin_cloud(name.split('.').next().unwrap_or(name)));
        others.extend(custom_mappers);
        let also = if others.is_empty() {
            String::new()
        } else {
            format!("; also configured on this device: {}", others.join(", "))
        };
        self.ui.line(&format!(
            "Warning: removing the shared device certificate {cert_path}{also}. \
             Other cloud connections using it will need to re-register"
        ));
    }

    /// Remove a file or directory, if present
    async fn remove(&self, path: &Utf8Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        if self.dry_run {
            self.detail(&format!("would remove {path}"));
            return Ok(());
        }
        if path.is_dir() {
            tokio::fs::remove_dir_all(path).await
        } else {
            tokio::fs::remove_file(path).await
        }
        .with_context(|| format!("Failed to remove {path}"))?;
        self.detail(&format!("removed {path}"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::read_config_string;
    use super::super::test_support::touch;
    use super::super::test_support::Fixture;
    use super::*;
    use crate::cli::bootstrap::mapper_toml::write_mapper_config;
    use crate::cli::bootstrap::settings::KeyValue;

    #[tokio::test]
    async fn clean_unwinds_only_the_bootstrap_managed_keys_of_a_builtin_cloud() {
        let fx = Fixture::new(
            r#"
[c8y]
url = "example.cumulocity.com"
auth_method = "basic"
credentials_path = "/etc/tedge/mappers/c8y/credentials.toml"
mqtt_service.enabled = true
software_management.api = "advanced"

[c8y.profiles.prod]
url = "prod.example.com"
"#,
        )
        .await;
        let mut command = fx.command(Cloud::c8y(None));
        command.clean = true;
        // the run's own settings are unwound too (they are re-applied);
        // a device-global --set is not the instance's to remove
        command.settings = vec![
            KeyValue::new("c8y.mqtt_service.enabled", "true"),
            KeyValue::new("proxy.address", "proxy.example.com"),
        ];

        let config = command
            .unwind_instance_config(fx.config().await)
            .await
            .unwrap();
        assert_eq!(read_config_string(&config, "c8y.url"), None);
        assert_eq!(
            read_config_string(&config, "c8y.auth_method").as_deref(),
            Some("certificate")
        );
        assert_eq!(
            read_config_string(&config, "c8y.mqtt_service.enabled").as_deref(),
            Some("false")
        );
        // the user's own settings of the section survive
        assert_eq!(
            read_config_string(&config, "c8y.software_management.api").as_deref(),
            Some("advanced")
        );
        // other instances are untouched
        assert_eq!(
            read_config_string(&config, "c8y.profiles.prod.url").as_deref(),
            Some("prod.example.com")
        );
    }

    #[tokio::test]
    async fn clean_unwinds_only_the_bootstrap_managed_keys_of_a_mapper_toml() {
        let fx = Fixture::new("").await;
        let path = MapperToml::path_for(&fx.config_dir, "acme");
        write_mapper_config(
            &path,
            &[
                KeyValue::new("url", "acme.example.com"),
                KeyValue::new("device.id", "acme01"),
                KeyValue::new("bridge.topic_prefix", "acme"),
                KeyValue::new("bridge.custom_rule", "keep-me"),
                KeyValue::new("transport.port", "8883"),
            ],
        )
        .await
        .unwrap();
        let mut command = fx.command(Cloud::Custom("acme".into()));
        command.clean = true;
        command.settings = vec![KeyValue::new("acme.transport.port", "8883")];

        command
            .unwind_instance_config(fx.config().await)
            .await
            .unwrap();
        let mapper_toml = MapperToml::load(&path).await.unwrap();
        assert_eq!(mapper_toml.url(), None);
        assert_eq!(mapper_toml.device_id(), None);
        assert_eq!(mapper_toml.get("bridge.topic_prefix"), None);
        assert_eq!(mapper_toml.get("transport.port"), None);
        // package-shipped content survives
        assert_eq!(mapper_toml.get_str("bridge.custom_rule"), Some("keep-me"));
    }

    #[tokio::test]
    async fn re_register_removes_the_instance_artifacts_and_stops_its_mapper() {
        let fx = Fixture::new("").await;
        let mut command = fx.command(Cloud::Custom("acme".into()));
        command.re_register = true;
        let config = fx.config().await;
        let credentials = fx.config_dir.join("mappers/acme/credentials.toml");
        let instance_cert = fx
            .config_dir
            .join("mappers/acme/device-certs/tedge-certificate.pem");
        let shared_cert: Utf8PathBuf = config.device_cert_path(None::<&Cloud>).unwrap().into();
        touch(&credentials);
        touch(&instance_cert);
        touch(&shared_cert);

        command
            .remove_registration_artifacts(&config)
            .await
            .unwrap();
        assert!(!credentials.exists());
        assert!(!instance_cert.exists());
        // a custom-named instance never removes the shared certificate
        assert!(shared_cert.exists());
        assert_eq!(
            *fx.services.calls.lock().unwrap(),
            vec!["stop tedge-mapper-acme", "disable tedge-mapper-acme"]
        );
    }
}
