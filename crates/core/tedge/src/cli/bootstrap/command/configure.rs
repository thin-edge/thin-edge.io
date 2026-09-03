//! The configure step: resolve the cloud endpoints and apply the configuration
//!
//! Built-in clouds are configured via tedge config keys
//! (for Cumulocity, the MQTT endpoint is discovered from the HTTP one);
//! custom mappers via their `mappers/<name>/mapper.toml`.

use super::apply_tedge_config_updates;
use super::BootstrapCommand;
use crate::cli::bootstrap::cli::KeyValue;
use crate::cli::bootstrap::cli::RegistrationMethod;
use crate::cli::bootstrap::mapper_toml::write_mapper_config;
use crate::cli::bootstrap::mapper_toml::MapperToml;
use crate::cli::bootstrap::tls::tls_trust_error;
use crate::cli::common::Cloud;
use anyhow::anyhow;
use anyhow::Context;
use certificate::CloudHttpConfig;
use std::time::Duration;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use url::Url;

/// The local proxy port of the default Cumulocity instance
const DEFAULT_PROXY_PORT: u16 = 8001;

/// How long the loginOptions query may take
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

impl BootstrapCommand {
    /// Resolve the cloud endpoints and apply all configuration updates
    pub(super) async fn configure(&self, config: TEdgeConfig) -> anyhow::Result<()> {
        if let Some(name) = self.custom_mapper_name() {
            return self.configure_custom_mapper(name, &config).await;
        }

        let mut updates: Vec<KeyValue> = Vec::new();
        if let Some(url) = &self.url {
            let http_url = normalize_http_url(url);
            let http_host = url_host(&http_url)?;
            match &self.cloud {
                #[cfg(feature = "c8y")]
                Cloud::C8y(_) if self.c8y_endpoints_resolved_without_url(&config) => {
                    // A prepare hook resolved the endpoints itself
                    // (c8y.http and c8y.mqtt are set while c8y.url is not):
                    // respect that resolution — persist the url for the
                    // remaining consumers and skip the loginOptions discovery
                    self.detail(
                        "c8y.http and c8y.mqtt are already resolved, skipping MQTT endpoint discovery",
                    );
                    updates.push(self.config_key("url", http_host));
                }
                #[cfg(feature = "c8y")]
                Cloud::C8y(_) if self.offline => {
                    // an offline run cannot query loginOptions;
                    // use the URL as-is, quietly (this is expected, not a
                    // degradation) - a later online run with --url re-discovers
                    self.detail("offline: skipping MQTT endpoint discovery, using the URL as-is");
                    updates.push(self.config_key("url", http_host));
                }
                #[cfg(feature = "c8y")]
                Cloud::C8y(_) => {
                    let http_config = config.cloud_root_certs().await?;
                    match discover_c8y_mqtt_host(&http_url, &http_config).await {
                        Ok(mqtt_host) if is_same_parent_domain(&mqtt_host, &http_host) => {
                            updates.push(self.config_key("url", http_host));
                        }
                        Ok(mqtt_host) => {
                            self.detail(&format!(
                                "discovered a dedicated MQTT endpoint: {mqtt_host} (HTTP: {http_host})"
                            ));
                            updates.push(self.config_key("http", http_host));
                            updates.push(self.config_key("mqtt", mqtt_host));
                        }
                        Err(err) => {
                            // A rejected certificate is not a discovery
                            // hiccup: the bridge and the proxy verify the
                            // platform against the same trust store, so
                            // carrying on would only defer the failure to
                            // `tedge connect`, in a less legible form
                            let source: &(dyn std::error::Error + 'static) = err.as_ref();
                            if let Some(err) =
                                tls_trust_error(source, &http_host, &self.trust_store(&config))
                            {
                                return Err(err);
                            }
                            self.ui.line(&format!(
                                "Warning: could not query {http_url}/tenant/loginOptions to discover \
                                 the MQTT endpoint ({err:#}); using the URL as-is"
                            ));
                            updates.push(self.config_key("url", http_host));
                        }
                    }
                }
                _ => updates.push(self.config_key("url", http_host)),
            }
        }
        for (key, value) in &self.method_settings {
            updates.push(self.config_key(key, value.clone()));
        }

        #[cfg(feature = "c8y")]
        if let Cloud::C8y(Some(profile)) = &self.cloud {
            updates.extend(self.c8y_profile_defaults(&config, profile).await?);
        }

        updates.extend(self.settings.iter().cloned());

        if updates.is_empty() {
            self.ui.debug("nothing to update");
            return Ok(());
        }
        self.report_updates(
            "",
            updates
                .iter()
                .map(|update| (update.key.as_str(), update.value.as_str())),
        );
        if !self.dry_run {
            apply_tedge_config_updates(config, &updates).await?;
        }
        Ok(())
    }

    /// A profiled Cumulocity instance must not clash with the default
    /// instance: its bridge topic prefix and local proxy port are
    /// defaulted per profile, and the certificate-based methods get
    /// per-profile cert/CSR paths (each tenant's CA signs its own
    /// certificate; the private key stays shared) —
    /// unless already configured or given with --set
    #[cfg(feature = "c8y")]
    async fn c8y_profile_defaults(
        &self,
        config: &TEdgeConfig,
        profile: &tedge_config::tedge_toml::ProfileName,
    ) -> anyhow::Result<Vec<KeyValue>> {
        let mut updates = Vec::new();
        let user_set = |suffix: &str| {
            let full = self.instance_key(suffix);
            self.settings.iter().any(|s| s.key == full)
        };
        if self
            .read_instance_setting(config, "bridge.topic_prefix")
            .is_none_or(|prefix| prefix == "c8y")
            && !user_set("bridge.topic_prefix")
        {
            updates.push(self.config_key("bridge.topic_prefix", format!("c8y-{profile}")));
            if !user_set("proxy.bind.port") {
                let port = next_free_c8y_proxy_port(config, &self.config_dir).await?;
                updates.push(self.config_key("proxy.bind.port", port.to_string()));
            }
        }

        if matches!(
            self.register,
            RegistrationMethod::C8yCa | RegistrationMethod::SelfSigned
        ) {
            let global_cert = config
                .device_cert_path(None::<&Cloud>)
                .map_err(anyhow::Error::new)?
                .to_string();
            if self
                .read_instance_setting(config, "device.cert_path")
                .is_none_or(|cert| cert == global_cert)
                && !user_set("device.cert_path")
            {
                let cert_dir = self
                    .mapper_dir(&format!("c8y.{profile}"))
                    .join("device-certs");
                updates.push(self.config_key(
                    "device.cert_path",
                    cert_dir.join("tedge-certificate.pem").to_string(),
                ));
                if !user_set("device.csr_path") {
                    updates.push(
                        self.config_key("device.csr_path", cert_dir.join("tedge.csr").to_string()),
                    );
                }
            }
        }
        Ok(updates)
    }

    /// Whether a prepare hook has resolved the Cumulocity endpoints itself:
    /// `c8y.http` and `c8y.mqtt` are explicitly set while `c8y.url` is not
    /// (with `c8y.url` unset, those two cannot be derived values)
    #[cfg(feature = "c8y")]
    fn c8y_endpoints_resolved_without_url(&self, config: &TEdgeConfig) -> bool {
        self.read_instance_setting(config, "url").is_none()
            && self.read_instance_setting(config, "http").is_some()
            && self.read_instance_setting(config, "mqtt").is_some()
    }

    /// Apply the URL and `--set` values to `mappers/<name>/mapper.toml`
    async fn configure_custom_mapper(
        &self,
        name: &str,
        config: &TEdgeConfig,
    ) -> anyhow::Result<()> {
        let mut updates: Vec<(String, String)> = Vec::new();
        if let Some(url) = &self.url {
            updates.push(("url".to_owned(), url.clone()));
        }
        // The device id doubles as the mapper's MQTT client id
        if let Some(device_id) = &self.device_id {
            updates.push(("device.id".to_owned(), device_id.clone()));
        }
        // Persist the instance's cloud type so re-runs
        // (and the mapper itself) know what this instance speaks
        if let Some(cloud_type) = &self.cloud_type {
            updates.push(("cloud_type".to_owned(), cloud_type.clone()));
        }

        // A named Cumulocity instance must not clash with the default
        // instance's bridge topic prefix and local proxy port;
        // default them per instance unless already configured or --set
        if self.is_c8y() {
            let existing = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
            let user_set = |suffix: &str| {
                let full = self.instance_key(suffix);
                self.settings.iter().any(|s| s.key == full)
            };
            if existing.get(&["bridge", "topic_prefix"]).is_none()
                && !user_set("bridge.topic_prefix")
            {
                updates.push(("bridge.topic_prefix".to_owned(), name.to_owned()));
            }
            if existing.get(&["proxy", "bind", "port"]).is_none() && !user_set("proxy.bind.port") {
                let port = next_free_c8y_proxy_port(config, &self.config_dir).await?;
                updates.push(("proxy.bind.port".to_owned(), port.to_string()));
            }
        }
        updates.extend(self.method_settings.iter().cloned());
        // Keys prefixed with the mapper name go to its mapper.toml;
        // unprefixed keys that are valid tedge config keys are
        // device-global settings (e.g. proxy.address) for the tedge config
        let mut global_updates: Vec<KeyValue> = Vec::new();
        for setting in &self.settings {
            match setting.key.strip_prefix(&format!("{name}.")) {
                Some(key) => updates.push((key.to_owned(), setting.value.clone())),
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
        self.report_updates(
            "",
            global_updates
                .iter()
                .map(|update| (update.key.as_str(), update.value.as_str())),
        );
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
        self.report_updates(&format!("{name}."), super::pairs(&updates));
        if !self.dry_run {
            write_mapper_config(&mapper_toml, &updates).await?;
        }
        Ok(())
    }
}

/// The next local proxy port not used by any existing Cumulocity instance:
/// the default instance, cloud profiles, and c8y-typed mapper directories
/// (instances without an explicit port count as using the default, 8001)
pub(super) async fn next_free_c8y_proxy_port(
    config: &TEdgeConfig,
    config_dir: &camino::Utf8Path,
) -> anyhow::Result<u16> {
    let mut used = std::collections::BTreeSet::new();
    let read_port = |key: String| -> Option<u16> {
        let key = key.parse::<ReadableKey>().ok()?;
        config.read_string(&key).ok()?.parse().ok()
    };
    used.insert(read_port("c8y.proxy.bind.port".to_owned()).unwrap_or(DEFAULT_PROXY_PORT));
    for profile in config.c8y_keys_str().flatten() {
        used.insert(
            read_port(format!("c8y.profiles.{profile}.proxy.bind.port"))
                .unwrap_or(DEFAULT_PROXY_PORT),
        );
    }
    let mappers = config_dir.join("mappers");
    if let Ok(mut entries) = tokio::fs::read_dir(&mappers).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = MapperToml::path_for(config_dir, &name);
            if !path.exists() {
                continue;
            }
            let mapper_toml = MapperToml::load_or_empty(&path).await;
            let c8y_like = name == "c8y"
                || name.starts_with("c8y.")
                || mapper_toml.cloud_type() == Some("c8y");
            if !c8y_like {
                continue;
            }
            let port = mapper_toml
                .get(&["proxy", "bind", "port"])
                .and_then(|value| value.as_integer())
                .and_then(|port| u16::try_from(port).ok())
                .unwrap_or(DEFAULT_PROXY_PORT);
            used.insert(port);
        }
    }
    (DEFAULT_PROXY_PORT..u16::MAX)
        .find(|port| !used.contains(port))
        .context("No free proxy port found")
}

/// Default to https:// when no scheme is given, and strip any trailing slash
pub(super) fn normalize_http_url(url: &str) -> String {
    let url = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("https://{url}")
    };
    url.trim_end_matches('/').to_owned()
}

pub(super) fn url_host(url: &str) -> anyhow::Result<String> {
    let parsed = Url::parse(url).with_context(|| format!("Invalid URL: {url}"))?;
    parsed
        .host_str()
        .map(|host| host.to_owned())
        .with_context(|| format!("URL has no host: {url}"))
}

/// Query the tenant's login options to discover the MQTT endpoint domain
///
/// The `self` link of the response points at the tenant's canonical domain,
/// which is the MQTT endpoint when the tenant is served
/// through a separate HTTP domain (e.g. behind an enterprise gateway).
async fn discover_c8y_mqtt_host(
    http_url: &str,
    http_config: &CloudHttpConfig,
) -> anyhow::Result<String> {
    let client = http_config.client_builder().build()?;
    let response: serde_json::Value = client
        .get(format!("{http_url}/tenant/loginOptions"))
        .timeout(DISCOVERY_TIMEOUT)
        .send()
        .await?
        .error_for_status()
        .context("The URL may not be correct, or may point to a non-Cumulocity instance")?
        .json()
        .await?;
    let self_link = response
        .get("self")
        .and_then(|v| v.as_str())
        .context("No self link in the loginOptions response")?;
    url_host(self_link)
}

/// Compare the parent domains (everything after the first label) of two hosts
fn is_same_parent_domain(a: &str, b: &str) -> bool {
    let parent = |host: &str| {
        host.split_once('.')
            .map(|(_, parent)| parent.to_owned())
            .unwrap_or_else(|| host.to_owned())
    };
    parent(a) == parent(b)
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
    }

    #[test]
    fn same_parent_domain_for_sibling_hosts() {
        assert!(is_same_parent_domain(
            "t1234.eu-latest.cumulocity.com",
            "other.eu-latest.cumulocity.com"
        ));
    }

    #[test]
    fn different_parent_domain_for_separate_http_endpoint() {
        assert!(!is_same_parent_domain(
            "mqtt.dm-zz-p.ioee10-cloud.com",
            "main.example.com"
        ));
    }

    #[tokio::test]
    async fn mqtt_host_is_discovered_from_the_login_options_self_link() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/tenant/loginOptions")
            .with_status(200)
            .with_body(r#"{"self": "https://t1234.eu-latest.cumulocity.com/tenant/loginOptions"}"#)
            .create_async()
            .await;
        let host = discover_c8y_mqtt_host(&server.url(), &CloudHttpConfig::test_value())
            .await
            .unwrap();
        assert_eq!(host, "t1234.eu-latest.cumulocity.com");
    }

    #[tokio::test]
    async fn non_cumulocity_endpoints_fail_discovery_with_guidance() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/tenant/loginOptions")
            .with_status(404)
            .create_async()
            .await;
        let err = discover_c8y_mqtt_host(&server.url(), &CloudHttpConfig::test_value())
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("may not be correct"), "{err:#}");
    }

    #[tokio::test]
    async fn next_proxy_port_skips_every_configured_c8y_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
        // the default instance keeps 8001, a profile took 8002 explicitly
        let config = TEdgeConfig::load_toml_str_with_root_dir(
            config_dir,
            r#"
[c8y.profiles.prod]
url = "prod.example.com"
proxy.bind.port = 8002
"#,
        );
        // a c8y-typed named instance without an explicit port counts as 8001,
        // a foreign mapper does not count at all
        write_mapper_config(
            &MapperToml::path_for(config_dir, "c8y-second"),
            &[("cloud_type".to_owned(), "c8y".to_owned())],
        )
        .await
        .unwrap();
        write_mapper_config(
            &MapperToml::path_for(config_dir, "acme"),
            &[("proxy.bind.port".to_owned(), "8003".to_owned())],
        )
        .await
        .unwrap();

        let port = next_free_c8y_proxy_port(&config, config_dir).await.unwrap();
        assert_eq!(port, 8003);
    }
}
