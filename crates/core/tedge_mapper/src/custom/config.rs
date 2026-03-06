//! Custom mapper configuration.
//!
//! A custom mapper's configuration is stored in `tedge.toml` within the mapper directory
//! (e.g. `/etc/tedge/mappers/custom.thingsboard/tedge.toml`). This file is optional —
//! it is only needed when the mapper establishes a cloud connection via the MQTT bridge.
//!
//! The full TOML table is available for `${mapper.*}` template expansion in bridge rules.

use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_config::models::HostPort;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::models::MQTT_TLS_PORT;

/// Authentication method for the cloud broker connection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethodConfig {
    /// Use certificate auth if cert/key are present, fall back to password auth if
    /// `credentials_path` is set.
    #[default]
    Auto,
    /// Mutual TLS — requires `device.cert_path` and `device.key_path`.
    Certificate,
    /// Username/password in the MQTT CONNECT packet — requires `credentials_path`.
    Password,
}

/// Parsed custom mapper `tedge.toml` configuration.
///
/// The `table` field holds the complete TOML document, available for `${mapper.*}`
/// template expansion in bridge rule files. The typed fields provide structured access
/// to the values needed to start the MQTT bridge.
#[derive(Debug)]
pub struct CustomMapperConfig {
    /// The complete TOML table — used for `${mapper.*}` template expansion.
    pub table: toml::Table,
    /// Cloud broker URL in `{host}:{port}` format (port defaults to 8883).
    pub url: Option<HostPort<MQTT_TLS_PORT>>,
    /// Device identity and TLS certificate settings.
    pub device: Option<DeviceConfig>,
    /// MQTT bridge settings (keepalive, clean session).
    pub bridge: BridgeConfig,
    /// Authentication method: auto, certificate, or password.
    pub auth_method: AuthMethodConfig,
    /// Path to a TOML credentials file for username/password authentication.
    /// The file must contain a `[credentials]` section with `username` and `password` fields.
    pub credentials_path: Option<Utf8PathBuf>,
}

/// Device identity and TLS settings.
#[derive(Debug, serde::Deserialize)]
pub struct DeviceConfig {
    /// Explicit MQTT client ID. Takes precedence over the certificate CN when both are set.
    pub id: Option<String>,
    /// Path to the client certificate file used to authenticate the device.
    pub cert_path: Option<Utf8PathBuf>,
    /// Path to the private key file for the client certificate.
    pub key_path: Option<Utf8PathBuf>,
    /// Path to the CA certificate used to verify the cloud broker's TLS certificate.
    /// If absent, the system trust store is used.
    pub root_cert_path: Option<Utf8PathBuf>,
}

/// MQTT bridge connection settings.
#[derive(Debug, Default, serde::Deserialize)]
pub struct BridgeConfig {
    /// Whether to use a clean MQTT session when connecting to the remote broker.
    /// Default: false (persistent session).
    #[serde(default)]
    pub clean_session: bool,
    /// MQTT keepalive interval
    pub keepalive_interval: Option<SecondsOrHumanTime>,
}

#[derive(Debug, serde::Deserialize)]
struct RawConfig {
    url: Option<HostPort<MQTT_TLS_PORT>>,
    device: Option<DeviceConfig>,
    #[serde(default)]
    bridge: BridgeConfig,
    #[serde(default)]
    auth_method: AuthMethodConfig,
    credentials_path: Option<Utf8PathBuf>,
}

/// Reads and parses `tedge.toml` from the given mapper directory.
///
/// Returns `Ok(None)` if the file does not exist (the mapper may be flows-only).
/// Returns an error with the file path if the file cannot be read or is not valid TOML.
pub async fn load_mapper_config(
    mapper_dir: &Utf8Path,
) -> anyhow::Result<Option<CustomMapperConfig>> {
    let config_path = mapper_dir.join("tedge.toml");

    let content = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| format!("Failed to read {config_path}"));
        }
    };

    let table: toml::Table = content
        .parse()
        .with_context(|| format!("Failed to parse {config_path}"))?;

    let raw: RawConfig = table
        .clone()
        .try_into()
        .with_context(|| format!("Invalid configuration in {config_path}"))?;

    let config = CustomMapperConfig {
        table,
        url: raw.url,
        device: raw.device,
        bridge: raw.bridge,
        auth_method: raw.auth_method,
        credentials_path: raw.credentials_path,
    };

    // Validate that cert_path and key_path are either both set or both absent.
    // A half-configured TLS identity silently falls back to no client auth,
    // causing a confusing cloud-side rejection.
    if let Some(device) = &config.device {
        match (&device.cert_path, &device.key_path) {
            (Some(_), None) => {
                anyhow::bail!(
                    "Invalid configuration in {config_path}: \
                     'device.cert_path' is set but 'device.key_path' is missing. \
                     Both must be provided for certificate authentication."
                );
            }
            (None, Some(_)) => {
                anyhow::bail!(
                    "Invalid configuration in {config_path}: \
                     'device.key_path' is set but 'device.cert_path' is missing. \
                     Both must be provided for certificate authentication."
                );
            }
            _ => {}
        }
    }

    Ok(Some(config))
}

/// Reads username and password from a credentials TOML file.
///
/// The file must contain a `[credentials]` section:
/// ```toml
/// [credentials]
/// username = "my-device"
/// password = "secret"
/// ```
pub fn read_mapper_credentials(credentials_path: &Utf8Path) -> anyhow::Result<(String, String)> {
    #[derive(serde::Deserialize)]
    struct CredentialsFile {
        credentials: BasicCredentials,
    }
    #[derive(serde::Deserialize)]
    struct BasicCredentials {
        username: String,
        password: String,
    }

    let contents = std::fs::read_to_string(credentials_path)
        .with_context(|| format!("Failed to read credentials file '{credentials_path}'"))?;
    let file: CredentialsFile = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse credentials file '{credentials_path}'"))?;
    Ok((file.credentials.username, file.credentials.password))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn returns_none_when_file_not_found() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.test");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        let result = load_mapper_config(&mapper_dir).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn parses_valid_toml() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"
url = "mqtt.thingsboard.io:8883"

[device]
cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"
key_path = "/etc/tedge/device-certs/tedge-private-key.pem"

[bridge]
topic_prefix = "tb"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();

        let url = config.url.unwrap();
        assert_eq!(url.host().to_string(), "mqtt.thingsboard.io");
        assert_eq!(url.port().0, 8883);

        let device = config.device.unwrap();
        assert_eq!(
            device.cert_path.unwrap(),
            Utf8PathBuf::from("/etc/tedge/device-certs/tedge-certificate.pem")
        );

        // Template table should contain bridge section too
        assert!(config.table.contains_key("bridge"));
    }

    #[tokio::test]
    async fn returns_error_with_path_for_invalid_toml() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.broken");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(mapper_dir.join("tedge.toml"), "this is not = valid [ toml")
            .await
            .unwrap();

        let err = load_mapper_config(&mapper_dir).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("tedge.toml"),
            "Error should mention the file path: {msg}"
        );
    }

    #[tokio::test]
    async fn default_port_is_8883_when_not_specified() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.noport");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"
url = "mqtt.example.com"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.url.unwrap().port().0, 8883);
    }

    #[tokio::test]
    async fn parses_full_schema() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.full");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"
url = "mqtt.example.com:8883"
auth_method = "certificate"
credentials_path = "/etc/tedge/mappers/custom.full/credentials.toml"

[device]
id = "my-device-id"
cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"
key_path = "/etc/tedge/device-certs/tedge-private-key.pem"
root_cert_path = "/etc/ssl/certs"

[bridge]
clean_session = true
keepalive_interval = "60s"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.auth_method, AuthMethodConfig::Certificate);
        assert_eq!(
            config.credentials_path.as_deref().map(|p| p.as_str()),
            Some("/etc/tedge/mappers/custom.full/credentials.toml")
        );
        let device = config.device.unwrap();
        assert_eq!(device.id.as_deref(), Some("my-device-id"));
        assert!(config.bridge.clean_session);
        assert!(config.bridge.keepalive_interval.is_some());
    }

    #[tokio::test]
    async fn parses_password_auth_method() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.pw");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"url = "mqtt.example.com"
auth_method = "password"
credentials_path = "/etc/tedge/mappers/custom.pw/creds.toml"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.auth_method, AuthMethodConfig::Password);
    }

    #[test]
    fn reads_credentials_from_toml_file() {
        let ttd = TempTedgeDir::new();
        let creds_path = ttd.utf8_path().join("credentials.toml");
        std::fs::write(
            &creds_path,
            "[credentials]\nusername = \"alice\"\npassword = \"s3cr3t\"\n",
        )
        .unwrap();

        let (username, password) = read_mapper_credentials(&creds_path).unwrap();
        assert_eq!(username, "alice");
        assert_eq!(password, "s3cr3t");
    }

    #[test]
    fn credentials_error_on_missing_file() {
        let ttd = TempTedgeDir::new();
        let creds_path = ttd.utf8_path().join("missing.toml");
        let err = read_mapper_credentials(&creds_path).unwrap_err();
        assert!(
            format!("{err}").contains("credentials"),
            "Error should mention credentials: {err}"
        );
    }

    #[tokio::test]
    async fn errors_when_cert_path_set_without_key_path() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.halfcert");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"url = "mqtt.example.com"
[device]
cert_path = "/etc/tedge/device-certs/tedge-certificate.pem"
"#,
        )
        .await
        .unwrap();

        let err = load_mapper_config(&mapper_dir).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cert_path") && msg.contains("key_path"),
            "Error should mention both cert_path and key_path: {msg}"
        );
    }

    #[tokio::test]
    async fn errors_when_key_path_set_without_cert_path() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/custom.halfkey");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("tedge.toml"),
            r#"url = "mqtt.example.com"
[device]
key_path = "/etc/tedge/device-certs/tedge-private-key.pem"
"#,
        )
        .await
        .unwrap();

        let err = load_mapper_config(&mapper_dir).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("key_path") && msg.contains("cert_path"),
            "Error should mention both key_path and cert_path: {msg}"
        );
    }
}
