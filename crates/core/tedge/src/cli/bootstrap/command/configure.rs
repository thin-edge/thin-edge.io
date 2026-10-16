//! The configure step: resolve the cloud endpoints and apply the configuration
//!
//! Built-in clouds are configured via tedge config keys,
//! custom mappers via their `mappers/<name>/mapper.toml`.
//! Cumulocity instances additionally get their MQTT endpoint discovered
//! and per-instance defaults applied (see [`crate::cli::bootstrap::c8y`]).

use super::apply_tedge_config_updates;
use super::BootstrapCommand;
use crate::cli::bootstrap::mapper_toml::write_mapper_config;
use crate::cli::bootstrap::settings::key;
use crate::cli::bootstrap::settings::KeyValue;
use anyhow::anyhow;
use anyhow::Context;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use url::Url;

impl BootstrapCommand {
    /// Resolve the cloud endpoints and apply all configuration updates
    pub(super) async fn configure(&self, config: TEdgeConfig) -> anyhow::Result<()> {
        if let Some(name) = self.custom_mapper_name() {
            return self.configure_custom_mapper(name, &config).await;
        }

        let mut updates: Vec<KeyValue> = Vec::new();
        if let Some(url) = &self.url {
            if self.is_c8y() {
                updates.extend(self.c8y_endpoint_updates(&config, url).await?);
            } else {
                let http_host = url_host(&normalize_http_url(url))?;
                updates.push(self.instance_setting(key::URL, http_host));
            }
        }
        updates.extend(
            self.method_settings
                .iter()
                .map(|setting| self.instance_setting(&setting.key, setting.value.clone())),
        );
        if self.is_c8y() {
            updates.extend(self.c8y_profile_defaults(&config).await?);
        }
        updates.extend(self.settings.iter().cloned());

        if updates.is_empty() {
            self.ui.debug("nothing to update");
            return Ok(());
        }
        self.report_updates("", &updates);
        if !self.dry_run {
            apply_tedge_config_updates(config, &updates).await?;
        }
        Ok(())
    }

    /// Apply the URL and `--set` values to `mappers/<name>/mapper.toml`
    async fn configure_custom_mapper(
        &self,
        name: &str,
        config: &TEdgeConfig,
    ) -> anyhow::Result<()> {
        let mut updates: Vec<KeyValue> = Vec::new();
        if let Some(url) = &self.url {
            updates.push(KeyValue::new(key::URL, url));
        }
        // The device id doubles as the mapper's MQTT client id
        if let Some(device_id) = &self.device_id {
            updates.push(KeyValue::new(key::DEVICE_ID, device_id));
        }
        // Persist the instance's cloud type so re-runs
        // (and the mapper itself) know what this instance speaks
        if let Some(cloud_type) = &self.cloud_type {
            updates.push(KeyValue::new(key::CLOUD_TYPE, cloud_type));
        }
        if self.is_c8y() {
            updates.extend(self.c8y_named_instance_defaults(name, config).await?);
        }
        updates.extend(self.method_settings.iter().cloned());

        // Keys prefixed with the mapper name go to its mapper.toml;
        // unprefixed keys that are valid tedge config keys are
        // device-global settings (e.g. proxy.address) for the tedge config
        let mut global_updates: Vec<KeyValue> = Vec::new();
        let own_prefix = format!("{name}.");
        for setting in &self.settings {
            match setting.key.strip_prefix(&own_prefix) {
                Some(key) => updates.push(KeyValue::new(key, &setting.value)),
                None if setting.key.parse::<WritableKey>().is_ok() => {
                    global_updates.push(setting.clone());
                }
                None => {
                    return Err(anyhow!(
                        "Custom mapper config keys must be prefixed with the mapper name \
                         (e.g. --set {name}.url=...), or be a valid device-global \
                         tedge config key (e.g. --set proxy.address=...); got {:?}",
                        setting.key
                    ));
                }
            }
        }
        self.report_updates("", &global_updates);
        if !global_updates.is_empty() && !self.dry_run {
            let fresh = self.load_config().await?;
            apply_tedge_config_updates(fresh, &global_updates).await?;
        }

        if updates.is_empty() {
            self.ui.debug("nothing to update");
            return Ok(());
        }
        let mapper_toml = self.mapper_config_path(name);
        self.detail(&format!("updating {mapper_toml}"));
        self.report_updates(&own_prefix, &updates);
        if !self.dry_run {
            write_mapper_config(&mapper_toml, &updates).await?;
        }
        Ok(())
    }
}

/// Default to https:// when no scheme is given, and strip any trailing slash
pub(in crate::cli::bootstrap) fn normalize_http_url(url: &str) -> String {
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    url.trim_end_matches('/').to_owned()
}

/// The host of a URL, with its port when one is given explicitly
pub(in crate::cli::bootstrap) fn url_host(url: &str) -> anyhow::Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("Invalid URL: {url}"))?;
    let host = parsed
        .host_str()
        .with_context(|| format!("URL has no host: {url}"))?;
    Ok(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_https_scheme() {
        assert_eq!(
            normalize_http_url("example.cumulocity.com"),
            "https://example.cumulocity.com"
        );
    }

    #[test]
    fn normalize_keeps_existing_scheme_and_strips_trailing_slash() {
        assert_eq!(
            normalize_http_url("http://example.cumulocity.com/"),
            "http://example.cumulocity.com"
        );
        assert_eq!(
            url_host("https://example.cumulocity.com/path").unwrap(),
            "example.cumulocity.com"
        );
        // an explicit port is part of the endpoint
        assert_eq!(
            url_host("https://example.cumulocity.com:8443/path").unwrap(),
            "example.cumulocity.com:8443"
        );
        assert!(url_host("https://").is_err());
    }
}
