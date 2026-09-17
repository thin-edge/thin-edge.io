//! The Cumulocity part of the configure step:
//! MQTT endpoint discovery, and the per-instance defaults
//! keeping several instances from clashing

use super::DEFAULT_PROXY_PORT;
use crate::cli::bootstrap::command::configure::normalize_http_url;
use crate::cli::bootstrap::command::configure::url_host;
use crate::cli::bootstrap::command::read_config_string;
use crate::cli::bootstrap::command::BootstrapCommand;
use crate::cli::bootstrap::mapper_toml::instance_cert_path;
use crate::cli::bootstrap::mapper_toml::instance_csr_path;
use crate::cli::bootstrap::mapper_toml::toml_nested;
use crate::cli::bootstrap::mapper_toml::MapperToml;
use crate::cli::bootstrap::resolve::RegistrationMethod;
use crate::cli::bootstrap::settings::key;
use crate::cli::bootstrap::settings::KeyValue;
use crate::cli::bootstrap::tls::tls_trust_failure;
use crate::cli::common::Cloud;
use anyhow::anyhow;
use anyhow::Context;
use c8y_api::registration::discover_tenant_domain;
use c8y_api::registration::LoginOptionsError;
use camino::Utf8Path;
use certificate::CloudHttpConfig;
use std::time::Duration;
use tedge_config::TEdgeConfig;
use tedge_mapper::custom_mapper_config::scan_mappers_shallow;

/// How long the loginOptions query may take
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);

impl BootstrapCommand {
    /// The endpoint settings of a built-in instance for the given URL:
    /// `url` alone when the MQTT endpoint shares its domain,
    /// else `http` and `mqtt` separately
    pub(in crate::cli::bootstrap) async fn c8y_endpoint_updates(
        &self,
        config: &TEdgeConfig,
        url: &str,
    ) -> anyhow::Result<Vec<KeyValue>> {
        let http_url = normalize_http_url(url);
        let http_host = url_host(&http_url)?;
        let url_only = |host: String| vec![self.instance_setting(key::URL, host)];

        if self.c8y_endpoints_resolved_without_url(config) {
            // A prepare hook resolved the endpoints itself
            // (c8y.http and c8y.mqtt are set while c8y.url is not):
            // respect that resolution — persist the url for the
            // remaining consumers and skip the loginOptions discovery
            self.detail(
                "c8y.http and c8y.mqtt are already resolved, skipping MQTT endpoint discovery",
            );
            return Ok(url_only(http_host));
        }
        if self.offline {
            // an offline run cannot query loginOptions;
            // use the URL as-is, quietly (this is expected, not a
            // degradation) - a later online run with --url re-discovers
            self.detail("offline: skipping MQTT endpoint discovery, using the URL as-is");
            return Ok(url_only(http_host));
        }
        let http_config = config.cloud_root_certs().await?;
        match discover_c8y_mqtt_host(&http_url, &http_config).await {
            Ok(mqtt_host) if is_same_parent_domain(&mqtt_host, &http_host) => {
                Ok(url_only(http_host))
            }
            Ok(mqtt_host) => {
                self.detail(&format!(
                    "discovered a dedicated MQTT endpoint: {mqtt_host} (HTTP: {http_host})"
                ));
                Ok(vec![
                    self.instance_setting(key::HTTP, http_host),
                    self.instance_setting(key::MQTT, mqtt_host),
                ])
            }
            Err(err) => {
                match tls_trust_failure(&err) {
                    // A dry run does not execute the prepare hooks, so trust
                    // a hook would have installed is not there yet:
                    // report it, but let the rehearsal run to its end
                    Some(failure) if self.dry_run => self.ui.line(&format!(
                        "Warning: {} - a prepare hook installing it \
                         has not run in this dry run",
                        failure.summary(&http_host)
                    )),
                    // A rejected certificate is not a discovery hiccup:
                    // the bridge and the proxy verify the platform against
                    // the same trust store, so carrying on would only defer
                    // the failure to `tedge connect`, in a less legible form
                    Some(failure) => {
                        let trust_store = self.trust_store(config).await;
                        return Err(anyhow!("{}", failure.explain(&http_host, &trust_store)));
                    }
                    None => self.ui.line(&format!(
                        "Warning: could not query {http_url}/tenant/loginOptions to \
                         discover the MQTT endpoint ({:#}); using the URL as-is",
                        anyhow::Error::from(err)
                    )),
                }
                Ok(url_only(http_host))
            }
        }
    }

    /// Whether a prepare hook has resolved the Cumulocity endpoints itself:
    /// `c8y.http` and `c8y.mqtt` are explicitly set while `c8y.url` is not
    /// (with `c8y.url` unset, those two cannot be derived values)
    fn c8y_endpoints_resolved_without_url(&self, config: &TEdgeConfig) -> bool {
        self.read_instance_setting(config, key::URL).is_none()
            && self.read_instance_setting(config, key::HTTP).is_some()
            && self.read_instance_setting(config, key::MQTT).is_some()
    }

    /// A profiled Cumulocity instance must not clash with the default
    /// instance: its bridge topic prefix and local proxy port are
    /// defaulted per profile, and the certificate-based methods get
    /// per-profile cert/CSR paths (each tenant's CA signs its own
    /// certificate; the private key stays shared) —
    /// unless already configured or given with --set
    pub(in crate::cli::bootstrap) async fn c8y_profile_defaults(
        &self,
        config: &TEdgeConfig,
    ) -> anyhow::Result<Vec<KeyValue>> {
        let Some(profile) = self.profile() else {
            return Ok(Vec::new());
        };
        let mut updates = Vec::new();
        if self
            .read_instance_setting(config, key::BRIDGE_TOPIC_PREFIX)
            .is_none_or(|prefix| prefix == super::CLOUD)
            && !self.user_set(key::BRIDGE_TOPIC_PREFIX)
        {
            updates.push(self.instance_setting(
                key::BRIDGE_TOPIC_PREFIX,
                format!("{}-{profile}", super::CLOUD),
            ));
            if !self.user_set(key::PROXY_BIND_PORT) {
                let port = next_free_c8y_proxy_port(config, &self.config_dir).await?;
                updates.push(self.instance_setting(key::PROXY_BIND_PORT, port.to_string()));
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
                .read_instance_setting(config, key::DEVICE_CERT_PATH)
                .is_none_or(|cert| cert == global_cert)
                && !self.user_set(key::DEVICE_CERT_PATH)
            {
                let mapper_dir = self.mapper_dir(&self.cloud.mapper_dir_name());
                updates.push(self.instance_setting(
                    key::DEVICE_CERT_PATH,
                    instance_cert_path(&mapper_dir).to_string(),
                ));
                if !self.user_set(key::DEVICE_CSR_PATH) {
                    updates.push(self.instance_setting(
                        key::DEVICE_CSR_PATH,
                        instance_csr_path(&mapper_dir).to_string(),
                    ));
                }
            }
        }
        Ok(updates)
    }

    /// A custom-named Cumulocity instance must not clash with the default
    /// instance's bridge topic prefix and local proxy port:
    /// default them per instance unless already configured or --set
    pub(in crate::cli::bootstrap) async fn c8y_named_instance_defaults(
        &self,
        name: &str,
        config: &TEdgeConfig,
    ) -> anyhow::Result<Vec<KeyValue>> {
        let existing = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
        let mut updates = Vec::new();
        if existing.get(key::BRIDGE_TOPIC_PREFIX).is_none()
            && !self.user_set(key::BRIDGE_TOPIC_PREFIX)
        {
            updates.push(KeyValue::new(key::BRIDGE_TOPIC_PREFIX, name));
        }
        if existing.get(key::PROXY_BIND_PORT).is_none() && !self.user_set(key::PROXY_BIND_PORT) {
            let port = next_free_c8y_proxy_port(config, &self.config_dir).await?;
            updates.push(KeyValue::new(key::PROXY_BIND_PORT, port.to_string()));
        }
        Ok(updates)
    }
}

/// The next local proxy port not used by any existing Cumulocity instance:
/// the default instance, cloud profiles, and c8y-typed mapper directories
/// (instances without an explicit port count as using the default)
async fn next_free_c8y_proxy_port(
    config: &TEdgeConfig,
    config_dir: &Utf8Path,
) -> anyhow::Result<u16> {
    let mut used = std::collections::BTreeSet::new();
    let read_port = |key: String| -> Option<u16> { read_config_string(config, &key)?.parse().ok() };
    let cloud = super::CLOUD;
    used.insert(
        read_port(format!("{cloud}.{}", key::PROXY_BIND_PORT)).unwrap_or(DEFAULT_PROXY_PORT),
    );
    for profile in config.c8y_keys_str().flatten() {
        used.insert(
            read_port(format!(
                "{cloud}.profiles.{profile}.{}",
                key::PROXY_BIND_PORT
            ))
            .unwrap_or(DEFAULT_PROXY_PORT),
        );
    }
    let mappers = scan_mappers_shallow(&config_dir.join("mappers")).await;
    for (name, mapper_toml) in mappers {
        let Some(mapper_toml) = mapper_toml else {
            continue;
        };
        let c8y_like = name == cloud
            || name.starts_with(&format!("{cloud}."))
            || toml_nested(&mapper_toml, key::CLOUD_TYPE).and_then(|v| v.as_str()) == Some(cloud);
        if !c8y_like {
            continue;
        }
        let port = toml_nested(&mapper_toml, key::PROXY_BIND_PORT)
            .and_then(|value| value.as_integer())
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(DEFAULT_PROXY_PORT);
        used.insert(port);
    }
    (DEFAULT_PROXY_PORT..u16::MAX)
        .find(|port| !used.contains(port))
        .context("No free proxy port found")
}

/// Query the tenant's login options to discover the MQTT endpoint domain
async fn discover_c8y_mqtt_host(
    http_url: &str,
    http_config: &CloudHttpConfig,
) -> Result<String, LoginOptionsError> {
    let client = http_config
        .client_builder()
        .timeout(DISCOVERY_TIMEOUT)
        .build()?;
    discover_tenant_domain(&client, http_url).await
}

/// Compare the parent domains (everything after the first label) of two hosts,
/// ignoring any port
fn is_same_parent_domain(a: &str, b: &str) -> bool {
    fn parent(host: &str) -> Option<&str> {
        let host = host.rsplit_once(':').map_or(host, |(host, _port)| host);
        host.split_once('.').map(|(_, parent)| parent)
    }
    parent(a) == parent(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::bootstrap::mapper_toml::write_mapper_config;

    #[test]
    fn same_parent_domain_for_sibling_hosts() {
        assert!(is_same_parent_domain(
            "t1234.eu-latest.cumulocity.com",
            "other.eu-latest.cumulocity.com:8443"
        ));
        assert!(!is_same_parent_domain(
            "mqtt.dm-zz-p.ioee10-cloud.com",
            "main.example.com"
        ));
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
            &[KeyValue::new("cloud_type", "c8y")],
        )
        .await
        .unwrap();
        write_mapper_config(
            &MapperToml::path_for(config_dir, "acme"),
            &[KeyValue::new("proxy.bind.port", "8003")],
        )
        .await
        .unwrap();

        let port = next_free_c8y_proxy_port(&config, config_dir).await.unwrap();
        assert_eq!(port, 8003);
    }
}
