//! Collects the (key, value) pairs a component is allowed to expose to external clients.
//!
//! These are pure functions over [TEdgeConfigReader], built on the same
//! [TEdgeConfigReader::readable_keys]/[TEdgeConfigReader::read_string] machinery used by
//! `tedge config list`. The caller (the agent for core config, a mapper for its own cloud
//! config) is responsible for publishing the result; this module only decides which keys are in
//! scope and what their current values are.

use super::ProfileName;
use super::ReadableKey;
use super::TEdgeConfigReader;
use crate::models::CloudType;

/// The exposable core (non-cloud) configuration, as (key, value) pairs.
///
/// A key with no value currently set is included with `None`, so the caller can still clear any
/// stale retained message for it.
pub fn exposed_core_config(reader: &TEdgeConfigReader) -> Vec<(String, Option<String>)> {
    reader
        .readable_keys()
        .filter(|key| key.is_exposable())
        .filter_map(|key| {
            let key_str = key.to_cow_str();
            (!is_cloud_key(&key_str)).then(|| {
                let value = reader.read_string(&key).ok();
                (key_str.into_owned(), value)
            })
        })
        .collect()
}

/// The exposable configuration owned by one cloud mapper instance, as (key, value) pairs with
/// the cloud (and profile) qualifier stripped, e.g. `c8y.url` becomes `url`.
///
/// A key with no value currently set is included with `None`, so the caller can still clear any
/// stale retained message for it.
pub fn exposed_cloud_config(
    reader: &TEdgeConfigReader,
    cloud: CloudType,
    profile: Option<&ProfileName>,
) -> anyhow::Result<Vec<(String, Option<String>)>> {
    let profile_str = profile.map(|p| p.to_string());
    let keys: Vec<ReadableKey> = match cloud {
        CloudType::C8y => reader
            .c8y_reader(profile_str.as_deref())?
            .readable_keys(profile_str.clone())
            .collect(),
        CloudType::Az => reader
            .az_reader(profile_str.as_deref())?
            .readable_keys(profile_str.clone())
            .collect(),
        CloudType::Aws => reader
            .aws_reader(profile_str.as_deref())?
            .readable_keys(profile_str.clone())
            .collect(),
    };

    let prefix = cloud_key_prefix(cloud, profile);
    Ok(keys
        .into_iter()
        .filter(|key| key.is_exposable())
        .filter_map(|key| {
            let key_str = key.to_cow_str();
            let local_key = key_str.strip_prefix(&prefix)?.to_owned();
            let value = reader.read_string(&key).ok();
            Some((local_key, value))
        })
        .collect())
}

fn is_cloud_key(key: &str) -> bool {
    [CloudType::C8y, CloudType::Az, CloudType::Aws]
        .iter()
        .any(|cloud| {
            key.starts_with(cloud.as_ref()) && key[cloud.as_ref().len()..].starts_with('.')
        })
}

fn cloud_key_prefix(cloud: CloudType, profile: Option<&ProfileName>) -> String {
    let cloud = cloud.as_ref();
    match profile {
        Some(profile) => format!("{cloud}.profiles.{profile}."),
        None => format!("{cloud}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEdgeConfigLocation;
    use camino::Utf8PathBuf;
    use std::str::FromStr;

    fn config_reader(toml: &str) -> TEdgeConfigReader {
        let dto = toml::from_str(toml).unwrap();
        let location = TEdgeConfigLocation::from_custom_root(Utf8PathBuf::from("/etc/tedge"));
        TEdgeConfigReader::from_dto(&dto, &location)
    }

    #[test]
    fn core_config_excludes_cloud_keys() {
        let reader = config_reader(
            r#"
            [device]
            type = "test-type"

            [c8y]
            url = "example.cumulocity.com"
            "#,
        );
        let core = exposed_core_config(&reader);
        assert!(core.iter().any(|(k, _)| k == "device.type"));
        assert!(!core.iter().any(|(k, _)| k.starts_with("c8y")));
    }

    #[test]
    fn core_config_excludes_c8y_entity_store_deprecated_alias() {
        let reader = config_reader("");
        let core = exposed_core_config(&reader);
        assert!(!core.iter().any(|(k, _)| k.starts_with("c8y.entity_store")));
        assert!(core
            .iter()
            .any(|(k, _)| k == "agent.entity_store.auto_register"));
    }

    #[test]
    fn core_config_reports_none_for_unset_exposable_keys() {
        let reader = config_reader("");
        // mqtt.client.host has a default, so it should always be set
        let core = exposed_core_config(&reader);
        let (_, host) = core.iter().find(|(k, _)| k == "mqtt.client.host").unwrap();
        assert_eq!(host.as_deref(), Some("127.0.0.1"));

        // c8y.url has no default, so with no config it should be unset
        let cloud = exposed_cloud_config(&reader, CloudType::C8y, None).unwrap();
        let (_, url) = cloud.iter().find(|(k, _)| k == "url").unwrap();
        assert_eq!(url, &None);
    }

    #[test]
    fn core_config_never_exposes_secrets() {
        let reader = config_reader(
            r#"
            [device]
            key_pin = "1234"

            [proxy]
            password = "hunter2"
            "#,
        );
        let core = exposed_core_config(&reader);
        assert!(!core.iter().any(|(k, _)| k == "device.key_pin"));
        assert!(!core.iter().any(|(k, _)| k == "proxy.password"));
    }

    #[test]
    fn cloud_config_strips_prefix_for_non_profiled_cloud() {
        let reader = config_reader(
            r#"
            [c8y]
            url = "example.cumulocity.com"
            "#,
        );
        let cloud = exposed_cloud_config(&reader, CloudType::C8y, None).unwrap();
        assert!(cloud
            .iter()
            .any(|(k, v)| k == "url" && v.as_deref() == Some("example.cumulocity.com")));
        assert!(!cloud.iter().any(|(k, _)| k.starts_with("c8y")));
    }

    #[test]
    fn cloud_config_strips_prefix_and_routes_profile() {
        let reader = config_reader(
            r#"
            [c8y.profiles.edge]
            url = "edge.c8y.io"
            "#,
        );
        let profile = ProfileName::from_str("edge").unwrap();
        let cloud = exposed_cloud_config(&reader, CloudType::C8y, Some(&profile)).unwrap();
        assert!(cloud
            .iter()
            .any(|(k, v)| k == "url" && v.as_deref() == Some("edge.c8y.io")));
    }

    #[test]
    fn cloud_config_never_exposes_secrets() {
        let reader = config_reader(
            r#"
            [c8y.device]
            key_pin = "1234"
            "#,
        );
        let cloud = exposed_cloud_config(&reader, CloudType::C8y, None).unwrap();
        assert!(!cloud.iter().any(|(k, _)| k == "device.key_pin"));
    }

    /// Forward-guarding test: if someone marks one of these known-secret keys
    /// `#[tedge_config(exposable)]`, this test fails CI. There is no value masking anywhere in
    /// this codebase, so the allowlist is the only thing standing between a secret and a retained
    /// MQTT message / HTTP response.
    #[test]
    fn known_secrets_are_never_exposable() {
        let secret_keys = [
            "device.key_pin",
            "device.key_uri",
            "device.key_path",
            "device.cert_path",
            "device.csr_path",
            "device.cryptoki.pin",
            "device.cryptoki.uri",
            "proxy.password",
            "proxy.username",
            "c8y.credentials_path",
            "c8y.device.key_pin",
            "c8y.device.key_uri",
            "c8y.device.key_path",
            "az.device.key_pin",
            "az.device.key_uri",
            "az.device.key_path",
            "aws.device.key_pin",
            "aws.device.key_uri",
            "aws.device.key_path",
            "mqtt.client.auth.cert_file",
            "mqtt.client.auth.key_file",
            "mqtt.client.auth.password_file",
            "http.client.auth.cert_file",
            "http.client.auth.key_file",
        ];

        for key in secret_keys {
            let parsed: ReadableKey = key
                .parse()
                .unwrap_or_else(|e| panic!("failed to parse known configuration key '{key}': {e}"));
            assert!(
                !parsed.is_exposable(),
                "'{key}' holds sensitive material and must never be marked #[tedge_config(exposable)]"
            );
        }
    }
}
