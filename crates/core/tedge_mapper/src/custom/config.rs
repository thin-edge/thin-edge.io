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

/// Parsed custom mapper `tedge.toml` configuration.
///
/// The `table` field holds the complete TOML document, available for `${mapper.*}`
/// template expansion in bridge rule files. The typed `connection` and `device` fields
/// provide structured access to the values needed to start the MQTT bridge.
#[derive(Debug)]
pub struct CustomMapperConfig {
    /// The complete TOML table — used for `${mapper.*}` template expansion.
    pub table: toml::Table,
    /// Connection settings for the cloud broker.
    pub connection: Option<ConnectionConfig>,
    /// Device identity and TLS certificate settings.
    pub device: Option<DeviceConfig>,
}

/// Cloud broker connection settings.
#[derive(Debug, serde::Deserialize)]
pub struct ConnectionConfig {
    /// Hostname or IP address of the cloud MQTT broker.
    pub url: String,
    /// Port number of the cloud MQTT broker (default: 8883).
    #[serde(default = "default_port")]
    pub port: u16,
}

/// Device identity and TLS settings.
#[derive(Debug, serde::Deserialize)]
pub struct DeviceConfig {
    /// Path to the client certificate file used to authenticate the device.
    pub cert_path: Option<Utf8PathBuf>,
    /// Path to the private key file for the client certificate.
    pub key_path: Option<Utf8PathBuf>,
    /// Path to the CA certificate used to verify the cloud broker's TLS certificate.
    /// If absent, the system trust store is used.
    pub root_cert_path: Option<Utf8PathBuf>,
}

fn default_port() -> u16 {
    8883
}

#[derive(Debug, serde::Deserialize)]
struct RawConfig {
    connection: Option<ConnectionConfig>,
    device: Option<DeviceConfig>,
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

    Ok(Some(CustomMapperConfig {
        table,
        connection: raw.connection,
        device: raw.device,
    }))
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
[connection]
url = "mqtt.thingsboard.io"
port = 8883

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

        let conn = config.connection.unwrap();
        assert_eq!(conn.url, "mqtt.thingsboard.io");
        assert_eq!(conn.port, 8883);

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
[connection]
url = "mqtt.example.com"
"#,
        )
        .await
        .unwrap();

        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        assert_eq!(config.connection.unwrap().port, 8883);
    }
}
