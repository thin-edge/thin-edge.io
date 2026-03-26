//! Mapper configuration.
//!
//! A mapper's configuration is stored in `mapper.toml` within the mapper directory
//! (e.g. `/etc/tedge/mappers/thingsboard/mapper.toml`). This file is optional —
//! it is only needed when the mapper establishes a cloud connection via the MQTT bridge.
//!
//! The full TOML table is available for `${mapper.*}` template expansion in bridge rules.

use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use tedge_config::models::CloudType;
use tedge_config::models::HostPort;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::models::MQTT_TLS_PORT;

/// Authentication method for the cloud broker connection.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
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

/// Parsed mapper `mapper.toml` configuration.
///
/// The `table` field holds the complete TOML document, available for `${mapper.*}`
/// template expansion in bridge rule files. The typed fields provide structured access
/// to the values needed to start the MQTT bridge.
///
/// ## Certificate fallback
///
/// When `device.cert_path` and `device.key_path` are absent from `mapper.toml`,
/// `build_cloud_mqtt_options` falls back to the values from the root `tedge.toml`
/// (`device.cert_path` / `device.key_path`). Explicit `mapper.toml` values always
/// take precedence over root `tedge.toml` values.
///
/// ## Relative paths
///
/// All path fields (`device.cert_path`, `device.key_path`, `device.root_cert_path`,
/// `credentials_path`) support relative paths. Relative paths are resolved relative
/// to the mapper directory (e.g. `/etc/tedge/mappers/thingsboard/`) at parse time,
/// so `cert.pem` in `mapper.toml` becomes `/etc/tedge/mappers/thingsboard/cert.pem`.
/// Absolute paths are returned unchanged.
#[derive(Debug)]
pub struct CustomMapperConfig {
    /// The complete TOML table — used for `${mapper.*}` template expansion.
    pub table: toml::Table,
    /// Cloud integration type, if this mapper delegates to a built-in cloud integration.
    pub cloud_type: Option<CloudType>,
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
#[derive(Debug, Clone, serde::Deserialize)]
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
#[derive(Debug, Default, Clone, serde::Deserialize)]
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
    cloud_type: Option<CloudType>,
    url: Option<HostPort<MQTT_TLS_PORT>>,
    device: Option<DeviceConfig>,
    #[serde(default)]
    bridge: BridgeConfig,
    #[serde(default)]
    auth_method: AuthMethodConfig,
    credentials_path: Option<Utf8PathBuf>,
}

/// Reads and parses `mapper.toml` from the given mapper directory.
///
/// Returns `Ok(None)` if the file does not exist (the mapper may be flows-only).
/// Returns an error with the file path if the file cannot be read or is not valid TOML.
pub async fn load_mapper_config(
    mapper_dir: &Utf8Path,
) -> anyhow::Result<Option<CustomMapperConfig>> {
    let config_path = mapper_dir.join("mapper.toml");

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
        .map_err(|e| anyhow::anyhow!("Invalid configuration in {config_path}: {e}"))?;

    // Resolve relative paths relative to the mapper directory so all downstream
    // code always sees absolute paths.
    let device = raw.device.map(|mut d| {
        d.cert_path = d.cert_path.map(|p| resolve_relative(mapper_dir, p));
        d.key_path = d.key_path.map(|p| resolve_relative(mapper_dir, p));
        d.root_cert_path = d.root_cert_path.map(|p| resolve_relative(mapper_dir, p));
        d
    });
    let credentials_path = raw
        .credentials_path
        .map(|p| resolve_relative(mapper_dir, p));

    let config = CustomMapperConfig {
        table,
        cloud_type: raw.cloud_type,
        url: raw.url,
        device,
        bridge: raw.bridge,
        auth_method: raw.auth_method,
        credentials_path,
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

/// Resolves `path` relative to `base` if it is relative; returns it unchanged if absolute.
fn resolve_relative(base: &Utf8Path, path: Utf8PathBuf) -> Utf8PathBuf {
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

/// Scans `mappers_root` and returns `(name, table)` for each subdirectory.
///
/// Every subdirectory under `mappers_root` is a potential mapper — a flows-only
/// mapper may have no `mapper.toml` before its first startup (the `flows/`
/// directory is created automatically when the mapper starts). If a `mapper.toml`
/// is present it is parsed and the resulting table returned; otherwise `None`.
///
/// Results are sorted alphabetically by mapper name.
pub async fn scan_mappers_shallow(mappers_root: &Utf8Path) -> Vec<(String, Option<toml::Table>)> {
    let Ok(mut entries) = tokio::fs::read_dir(mappers_root).await else {
        return Vec::new();
    };

    let mut mappers = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(ft) = entry.file_type().await else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        let path = Utf8PathBuf::from(entry.path().to_string_lossy().into_owned());
        let name = entry.file_name().to_string_lossy().into_owned();
        let table = read_mapper_toml_table(&path).await;
        mappers.push((name, table));
    }
    mappers.sort_by(|(a, _), (b, _)| a.cmp(b));
    mappers
}

/// Reads `mapper.toml` from the given mapper directory and parses it as a TOML table.
/// Returns `None` if the file does not exist or cannot be parsed.
async fn read_mapper_toml_table(mapper_dir: &Utf8Path) -> Option<toml::Table> {
    let content = tokio::fs::read_to_string(mapper_dir.join("mapper.toml"))
        .await
        .ok()?;
    content.parse().ok()
}

/// Reads username and password from a credentials TOML file.
///
/// The file must contain a `[credentials]` section:
/// ```toml
/// [credentials]
/// username = "my-device"
/// password = "secret"
/// ```
pub async fn read_mapper_credentials(
    credentials_path: &Utf8Path,
) -> anyhow::Result<(String, String)> {
    #[derive(serde::Deserialize)]
    struct CredentialsFile {
        credentials: BasicCredentials,
    }
    #[derive(serde::Deserialize)]
    struct BasicCredentials {
        username: String,
        password: String,
    }

    let contents = tokio::fs::read_to_string(credentials_path)
        .await
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
    async fn cloud_type_parses_known_values() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        for (toml_val, expected) in [
            ("c8y", tedge_config::models::CloudType::C8y),
            ("az", tedge_config::models::CloudType::Az),
            ("aws", tedge_config::models::CloudType::Aws),
        ] {
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                format!("cloud_type = \"{toml_val}\"\n"),
            )
            .await
            .unwrap();
            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            assert_eq!(config.cloud_type, Some(expected));
        }
    }

    #[tokio::test]
    async fn cloud_type_rejects_unknown_value() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            "cloud_type = \"notacloud\"\n",
        )
        .await
        .unwrap();

        let err = load_mapper_config(&mapper_dir).await.unwrap_err();
        assert!(
            format!("{err}").contains("notacloud") || format!("{err}").contains("cloud_type"),
            "error should mention the invalid value: {err}"
        );
    }

    #[tokio::test]
    async fn cloud_type_absent_gives_none() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            "url = \"mqtt.example.com:8883\"\n",
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.cloud_type, None);
    }

    #[tokio::test]
    async fn returns_none_when_file_not_found() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/testmapper");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        let result = load_mapper_config(&mapper_dir).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn parses_valid_toml() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
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
        let mapper_dir = ttd.utf8_path().join("mappers/broken");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(mapper_dir.join("mapper.toml"), "this is not = valid [ toml")
            .await
            .unwrap();

        let err = load_mapper_config(&mapper_dir).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("mapper.toml"),
            "Error should mention the file path: {msg}"
        );
    }

    #[tokio::test]
    async fn default_port_is_8883_when_not_specified() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/noport");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
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
        let mapper_dir = ttd.utf8_path().join("mappers/full");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            r#"
url = "mqtt.example.com:8883"
auth_method = "certificate"
credentials_path = "/etc/tedge/mappers/full/credentials.toml"

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
            Some("/etc/tedge/mappers/full/credentials.toml")
        );
        let device = config.device.unwrap();
        assert_eq!(device.id.as_deref(), Some("my-device-id"));
        assert!(config.bridge.clean_session);
        assert!(config.bridge.keepalive_interval.is_some());
    }

    #[tokio::test]
    async fn parses_password_auth_method() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/pw");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();

        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            r#"url = "mqtt.example.com"
auth_method = "password"
credentials_path = "/etc/tedge/mappers/pw/creds.toml"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.auth_method, AuthMethodConfig::Password);
    }

    #[tokio::test]
    async fn reads_credentials_from_toml_file() {
        let ttd = TempTedgeDir::new();
        let creds_path = ttd.utf8_path().join("credentials.toml");
        tokio::fs::write(
            &creds_path,
            "[credentials]\nusername = \"alice\"\npassword = \"s3cr3t\"\n",
        )
        .await
        .unwrap();

        let (username, password) = read_mapper_credentials(&creds_path).await.unwrap();
        assert_eq!(username, "alice");
        assert_eq!(password, "s3cr3t");
    }

    #[tokio::test]
    async fn credentials_error_on_missing_file() {
        let ttd = TempTedgeDir::new();
        let creds_path = ttd.utf8_path().join("missing.toml");
        let err = read_mapper_credentials(&creds_path).await.unwrap_err();
        assert!(
            format!("{err}").contains("credentials"),
            "Error should mention credentials: {err}"
        );
    }

    #[tokio::test]
    async fn errors_when_cert_path_set_without_key_path() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/halfcert");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
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
        let mapper_dir = ttd.utf8_path().join("mappers/halfkey");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
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

    mod relative_paths {
        use super::*;

        #[tokio::test]
        async fn relative_cert_path_is_resolved_to_mapper_dir() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
            )
            .await
            .unwrap();

            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let device = config.device.unwrap();
            assert_eq!(
                device.cert_path.unwrap(),
                mapper_dir.join("cert.pem"),
                "Relative cert_path should resolve to mapper_dir/cert.pem"
            );
            assert_eq!(
                device.key_path.unwrap(),
                mapper_dir.join("key.pem"),
                "Relative key_path should resolve to mapper_dir/key.pem"
            );
        }

        #[tokio::test]
        async fn absolute_cert_path_is_unchanged() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[device]\ncert_path = \"/etc/tedge/device-certs/tedge-certificate.pem\"\nkey_path = \"/etc/tedge/device-certs/tedge-private-key.pem\"\n",
            )
            .await
            .unwrap();

            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let device = config.device.unwrap();
            assert_eq!(
                device.cert_path.unwrap(),
                Utf8PathBuf::from("/etc/tedge/device-certs/tedge-certificate.pem"),
                "Absolute cert_path should be returned unchanged"
            );
        }

        #[tokio::test]
        async fn nested_relative_path_resolves_correctly() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "[device]\ncert_path = \"certs/device.pem\"\nkey_path = \"certs/device.key\"\n",
            )
            .await
            .unwrap();

            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            let device = config.device.unwrap();
            assert_eq!(
                device.cert_path.unwrap(),
                mapper_dir.join("certs/device.pem")
            );
        }

        #[tokio::test]
        async fn relative_credentials_path_is_resolved() {
            let ttd = TempTedgeDir::new();
            let mapper_dir = ttd.utf8_path().join("mappers/thingsboard");
            tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
            tokio::fs::write(
                mapper_dir.join("mapper.toml"),
                "credentials_path = \"credentials.toml\"\n",
            )
            .await
            .unwrap();

            let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
            assert_eq!(
                config.credentials_path.unwrap(),
                mapper_dir.join("credentials.toml")
            );
        }
    }
}
