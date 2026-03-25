//! Effective mapper configuration resolution.
//!
//! This module provides [`resolve_effective_config`], which applies all fallback
//! and inference logic to produce an [`EffectiveMapperConfig`] — the configuration
//! the mapper will actually use at runtime. The resolution logic is shared between
//! the mapper runtime and the `tedge mapper config get` / `tedge mapper list` CLI
//! commands so that both reflect exactly the same values.

use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use tedge_config::models::HostPort;
use tedge_config::models::MQTT_TLS_PORT;
use tedge_config::tedge_toml::Cloud;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::config_toml::AuthMethod;

use crate::custom::config::AuthMethodConfig;
use crate::custom::config::BridgeConfig;
use crate::custom::config::CustomMapperConfig;

/// Tracks the origin of a resolved configuration value.
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// Value was explicitly set in `mapper.toml`.
    MapperToml,
    /// Value was a relative path in `mapper.toml`; the stored value is the resolved
    /// absolute form. The original relative string is preserved for display.
    MapperTomlResolved { original: String },
    /// Value was not set in `mapper.toml` and was inherited from the root `tedge.toml`.
    TedgeToml,
    /// Value was inferred from the Subject Common Name of the device certificate.
    CertificateCN { cert_path: Utf8PathBuf },
    /// Value is the schema default (not present in any configuration file).
    Default,
}

impl ConfigSource {
    /// Returns a short tag suitable for tabular output (e.g. `tedge mapper list`).
    pub fn short_tag(&self) -> &'static str {
        match self {
            ConfigSource::MapperToml | ConfigSource::MapperTomlResolved { .. } => "mapper.toml",
            ConfigSource::TedgeToml => "tedge.toml",
            ConfigSource::CertificateCN { .. } => "cert CN",
            ConfigSource::Default => "default",
        }
    }
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::MapperToml => write!(f, "from mapper.toml"),
            ConfigSource::MapperTomlResolved { original } => write!(
                f,
                "relative path '{original}' in mapper.toml, resolved to absolute"
            ),
            ConfigSource::TedgeToml => {
                write!(f, "not set in mapper.toml, inherited from tedge.toml")
            }
            ConfigSource::CertificateCN { cert_path } => {
                write!(f, "inferred from certificate CN ({cert_path})")
            }
            ConfigSource::Default => write!(f, "schema default"),
        }
    }
}

/// A resolved configuration value together with its origin.
#[derive(Debug, Clone)]
pub struct Sourced<T> {
    pub value: T,
    pub source: ConfigSource,
}

/// The fully resolved effective configuration for a mapper instance.
///
/// Produced by [`resolve_effective_config`]. All path fields are absolute and all
/// fallbacks from the root `tedge.toml` have been applied.
///
/// `device_id` is `None` when cert auth is in use but the certificate cannot be
/// read — the mapper also cannot start in that state, so there is no honest value
/// to display.
#[derive(Debug)]
pub struct EffectiveMapperConfig {
    /// Cloud broker URL (from `mapper.toml`), if configured.
    pub url: Option<Sourced<HostPort<MQTT_TLS_PORT>>>,
    /// Effective MQTT client ID. `None` when cert auth is in use but the cert is
    /// unreadable.
    pub device_id: Option<Sourced<String>>,
    /// TLS client certificate path (`mapper.toml` → `tedge.toml` fallback).
    pub cert_path: Option<Sourced<Utf8PathBuf>>,
    /// TLS client private key path (`mapper.toml` → `tedge.toml` fallback).
    pub key_path: Option<Sourced<Utf8PathBuf>>,
    /// CA certificate path for verifying the cloud broker (`mapper.toml` → `/etc/ssl/certs`).
    pub root_cert_path: Sourced<Utf8PathBuf>,
    /// Credentials file path for password authentication (`mapper.toml` only).
    pub credentials_path: Option<Sourced<Utf8PathBuf>>,
    /// Resolved effective authentication method (after `auto` expansion).
    pub effective_auth: AuthMethod,
    /// MQTT bridge settings (keepalive interval, clean session).
    pub bridge: BridgeConfig,
    /// Raw TOML table from `mapper.toml`, used for custom key lookups.
    pub table: toml::Table,
}

/// Resolves a [`CustomMapperConfig`] into an [`EffectiveMapperConfig`].
///
/// Resolution order per field:
/// - `cert_path` / `key_path`: `mapper.toml` → root `tedge.toml`
/// - `root_cert_path`: `mapper.toml` → `/etc/ssl/certs` (default)
/// - `device_id` (cert auth): cert CN → explicit `device.id` → root `tedge.toml`
/// - `device_id` (cert auth, unreadable cert): `None` (no fallback — avoids misleading output)
/// - `device_id` (password auth): explicit `device.id` → root `tedge.toml`
///
/// Missing path values are returned as `None`; callers are responsible for failing
/// if a required field is absent.
pub async fn resolve_effective_config(
    config: &CustomMapperConfig,
    tedge_config: &TEdgeConfig,
) -> anyhow::Result<EffectiveMapperConfig> {
    let effective_auth = match config.auth_method {
        AuthMethodConfig::Certificate => AuthMethod::Certificate,
        AuthMethodConfig::Password => AuthMethod::Password,
        AuthMethodConfig::Auto => {
            if config.credentials_path.is_some() {
                AuthMethod::Password
            } else {
                AuthMethod::Certificate
            }
        }
    };

    let url = config.url.clone().map(|u| Sourced {
        value: u,
        source: ConfigSource::MapperToml,
    });

    let cert_path = resolve_path_sourced(
        config.device.as_ref().and_then(|d| d.cert_path.as_ref()),
        &config.table,
        &["device", "cert_path"],
        || {
            tedge_config
                .device_cert_path(None::<tedge_config::tedge_toml::tedge_config::Cloud<'_>>)
                .ok()
                .map(|p| p.into())
        },
    );

    let key_path = resolve_path_sourced(
        config.device.as_ref().and_then(|d| d.key_path.as_ref()),
        &config.table,
        &["device", "key_path"],
        || {
            tedge_config
                .device_key_path(None::<tedge_config::tedge_toml::tedge_config::Cloud<'_>>)
                .ok()
                .map(|p| p.into())
        },
    );

    let root_cert_path = resolve_path_sourced(
        config
            .device
            .as_ref()
            .and_then(|d| d.root_cert_path.as_ref()),
        &config.table,
        &["device", "root_cert_path"],
        || None,
    )
    .unwrap_or_else(|| Sourced {
        value: Utf8PathBuf::from("/etc/ssl/certs"),
        source: ConfigSource::Default,
    });

    let credentials_path = config.credentials_path.clone().map(|p| Sourced {
        value: p,
        source: ConfigSource::MapperToml,
    });

    let device_id =
        resolve_device_id(config, tedge_config, &effective_auth, cert_path.as_ref()).await;

    Ok(EffectiveMapperConfig {
        url,
        device_id,
        cert_path,
        key_path,
        root_cert_path,
        credentials_path,
        effective_auth,
        bridge: config.bridge.clone(),
        table: config.table.clone(),
    })
}

/// Resolves a path field, preserving information about whether it was a relative
/// path in `mapper.toml`. Uses the already-resolved (absolute) path from the
/// parsed `CustomMapperConfig` and checks the raw TOML table to determine whether
/// the original was relative.
fn resolve_path_sourced(
    resolved: Option<&Utf8PathBuf>,
    table: &toml::Table,
    table_key_path: &[&str],
    tedge_fallback: impl FnOnce() -> Option<Utf8PathBuf>,
) -> Option<Sourced<Utf8PathBuf>> {
    match resolved {
        Some(path) => {
            let original_str = walk_table_str(table, table_key_path);
            let source = match original_str {
                Some(s) if !Utf8Path::new(s).is_absolute() => ConfigSource::MapperTomlResolved {
                    original: s.to_string(),
                },
                _ => ConfigSource::MapperToml,
            };
            Some(Sourced {
                value: path.clone(),
                source,
            })
        }
        None => tedge_fallback().map(|p| Sourced {
            value: p,
            source: ConfigSource::TedgeToml,
        }),
    }
}

/// Walks a nested TOML table to retrieve a string value at the given key path.
fn walk_table_str<'a>(table: &'a toml::Table, keys: &[&str]) -> Option<&'a str> {
    let mut current = table;
    let (last, rest) = keys.split_last()?;
    for key in rest {
        current = current.get(*key)?.as_table()?;
    }
    current.get(*last)?.as_str()
}

/// Resolves the effective MQTT client ID with source annotation.
///
/// For certificate auth:
/// - Cert readable with non-empty CN → use CN (source: `CertificateCN`)
/// - Cert readable but no CN → fall through to explicit `device.id` or `tedge.toml`
/// - Cert unreadable → `None` (mapper will also fail at runtime; don't show a
///   misleading value)
///
/// For password auth: explicit `device.id` → `tedge.toml` device_id → `None`
async fn resolve_device_id(
    config: &CustomMapperConfig,
    tedge_config: &TEdgeConfig,
    effective_auth: &AuthMethod,
    cert_path: Option<&Sourced<Utf8PathBuf>>,
) -> Option<Sourced<String>> {
    let explicit_id = config.device.as_ref().and_then(|d| d.id.clone());
    let tedge_id = tedge_config
        .device_id(None::<Cloud<'_>>)
        .ok()
        .filter(|id| !id.is_empty());

    match effective_auth {
        AuthMethod::Certificate => {
            let cert = cert_path?.value.clone();
            match PemCertificate::from_pem_file(&cert).and_then(|c| c.subject_common_name()) {
                Ok(cn) if !cn.is_empty() => Some(Sourced {
                    value: cn,
                    source: ConfigSource::CertificateCN {
                        cert_path: cert.clone(),
                    },
                }),
                Ok(_) => {
                    // Cert readable but no CN — fall through to explicit id / tedge.toml
                    explicit_id
                        .map(|id| Sourced {
                            value: id,
                            source: ConfigSource::MapperToml,
                        })
                        .or_else(|| {
                            tedge_id.map(|id| Sourced {
                                value: id,
                                source: ConfigSource::TedgeToml,
                            })
                        })
                }
                Err(_) => {
                    // Cert unreadable — return None so callers don't show a misleading
                    // value. The mapper cannot start without a readable cert either.
                    None
                }
            }
        }
        AuthMethod::Password => explicit_id
            .map(|id| Sourced {
                value: id,
                source: ConfigSource::MapperToml,
            })
            .or_else(|| {
                tedge_id.map(|id| Sourced {
                    value: id,
                    source: ConfigSource::TedgeToml,
                })
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom::config::load_mapper_config;
    use crate::custom::config::BridgeConfig;
    use crate::custom::config::DeviceConfig;
    use camino::Utf8PathBuf;
    use tedge_config::TEdgeConfig;
    use tedge_test_utils::fs::TempTedgeDir;

    // Test EC certificate (CN = "localhost") and matching private key.
    // Same constants used in mapper.rs tests.
    const TEST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIIBnzCCAUWgAwIBAgIUSTUtJUfUdERMKBwsfdRv9IbvQicwCgYIKoZIzj0EAwIw\n\
FDESMBAGA1UEAwwJbG9jYWxob3N0MCAXDTIzMTExNDE2MDUwOVoYDzMwMjMwMzE3\n\
MTYwNTA5WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwWTATBgcqhkjOPQIBBggqhkjO\n\
PQMBBwNCAAR2SVEPD34AAxFuk0xYm60p7hA7+1SW+sFHazBRg32ifFd0o2Mn+Tf+\n\
voYflBi3v4lhr361RoWB8QfmaGN05vv+o3MwcTAdBgNVHQ4EFgQUAb4jQ7RQ/xyg\n\
cZM+We8ik29/oxswHwYDVR0jBBgwFoAUAb4jQ7RQ/xygcZM+We8ik29/oxswIQYD\n\
VR0RBBowGIIJbG9jYWxob3N0ggsqLmxvY2FsaG9zdDAMBgNVHRMBAf8EAjAAMAoG\n\
CCqGSM49BAMCA0gAMEUCIA6QrxoDHQJqoly7d8VN0sj0eDvfFpbbZdSnzBd6R8AP\n\
AiEAm/PAH3IPGuHRBIpdC0rNR8F/l3WcN9I9984qKZdG5rs=\n\
-----END CERTIFICATE-----\n";

    const TEST_KEY_PEM: &str = "\
-----BEGIN EC PRIVATE KEY-----\n\
MHcCAQEEIBX2Z/NKGEX14QbH4kb5GXom0pqSPfX0mxdWbLb86apEoAoGCCqGSM49\n\
AwEHoUQDQgAEdklRDw9+AAMRbpNMWJutKe4QO/tUlvrBR2swUYN9onxXdKNjJ/k3\n\
/r6GH5QYt7+JYa9+tUaFgfEH5mhjdOb7/g==\n\
-----END EC PRIVATE KEY-----\n";

    async fn write_cert(dir: &Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        tokio::fs::write(&cert, TEST_CERT_PEM).await.unwrap();
        tokio::fs::write(&key, TEST_KEY_PEM).await.unwrap();
        (cert, key)
    }

    async fn write_cert_no_cn(dir: &Utf8Path) -> (Utf8PathBuf, Utf8PathBuf) {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let mut params = rcgen::CertificateParams::default();
        params.distinguished_name = rcgen::DistinguishedName::new();
        let issuer = rcgen::Issuer::from_params(&params, &key);
        let cert = params.signed_by(&key, &issuer).unwrap();
        let cert_path = dir.join("cert-no-cn.pem");
        let key_path = dir.join("key-no-cn.pem");
        tokio::fs::write(&cert_path, cert.pem()).await.unwrap();
        tokio::fs::write(&key_path, key.serialize_pem())
            .await
            .unwrap();
        (cert_path, key_path)
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

    #[tokio::test]
    async fn cert_cn_used_as_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "localhost");
        assert!(matches!(id.source, ConfigSource::CertificateCN { .. }));
    }

    #[tokio::test]
    async fn explicit_device_id_overridden_by_cert_cn() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.device = Some(DeviceConfig {
            id: Some("explicit-id".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        // CN takes precedence over explicit id
        assert_eq!(effective.device_id.unwrap().value, "localhost");
    }

    #[tokio::test]
    async fn cert_with_no_cn_falls_back_to_explicit_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert_no_cn(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.device = Some(DeviceConfig {
            id: Some("fallback-device".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "fallback-device");
        assert!(matches!(id.source, ConfigSource::MapperToml));
    }

    #[tokio::test]
    async fn unreadable_cert_yields_none_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let tedge_config = TEdgeConfig::load_toml_str(
            "device.cert_path = \"/nonexistent/cert.pem\"\n\
             device.key_path = \"/nonexistent/key.pem\"\n",
        );
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        assert!(
            effective.device_id.is_none(),
            "device_id should be None when cert is unreadable"
        );
    }

    #[tokio::test]
    async fn password_auth_uses_explicit_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let creds_path = ttd.utf8_path().join("creds.toml");
        tokio::fs::write(
            &creds_path,
            "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
        )
        .await
        .unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.credentials_path = Some(creds_path);
        config.device = Some(DeviceConfig {
            id: Some("my-device".to_string()),
            cert_path: None,
            key_path: None,
            root_cert_path: None,
        });

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "my-device");
        assert!(matches!(id.source, ConfigSource::MapperToml));
    }

    #[tokio::test]
    async fn password_auth_falls_back_to_tedge_device_id() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let creds_path = ttd.utf8_path().join("creds.toml");
        tokio::fs::write(
            &creds_path,
            "[credentials]\nusername = \"u\"\npassword = \"p\"\n",
        )
        .await
        .unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("device.id = \"root-device\"");
        let mut config = make_config(Some("mqtt.example.com:1883"));
        config.credentials_path = Some(creds_path);

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        let id = effective.device_id.unwrap();
        assert_eq!(id.value, "root-device");
        assert!(matches!(id.source, ConfigSource::TedgeToml));
    }

    #[tokio::test]
    async fn cert_path_from_mapper_toml_is_sourced_correctly() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let toml = format!("[device]\ncert_path = \"{cert}\"\nkey_path = \"{key}\"\n");
        tokio::fs::write(mapper_dir.join("mapper.toml"), &toml)
            .await
            .unwrap();
        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        assert!(matches!(
            effective.cert_path.unwrap().source,
            ConfigSource::MapperToml
        ));
    }

    #[tokio::test]
    async fn relative_cert_path_annotated_as_resolved() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        // write actual cert files so load_mapper_config doesn't fail on validation
        let (_, _) = write_cert(&mapper_dir).await;
        tokio::fs::write(
            mapper_dir.join("mapper.toml"),
            "[device]\ncert_path = \"cert.pem\"\nkey_path = \"key.pem\"\n",
        )
        .await
        .unwrap();
        let config = load_mapper_config(&mapper_dir).await.unwrap().unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        let cert = effective.cert_path.unwrap();
        // Absolute path returned
        assert!(cert.value.is_absolute());
        // Source annotated as relative→resolved
        assert!(
            matches!(cert.source, ConfigSource::MapperTomlResolved { ref original } if original == "cert.pem"),
            "expected MapperTomlResolved with original='cert.pem', got {:?}",
            cert.source
        );
    }

    #[tokio::test]
    async fn cert_path_falls_back_to_tedge_toml() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let (cert, key) = write_cert(ttd.utf8_path()).await;
        let tedge_config = TEdgeConfig::load_toml_str(&format!(
            "device.cert_path = \"{cert}\"\ndevice.key_path = \"{key}\"\n"
        ));
        // No cert_path in mapper.toml
        let config = make_config(Some("mqtt.example.com:1883"));

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        assert!(matches!(
            effective.cert_path.unwrap().source,
            ConfigSource::TedgeToml
        ));
    }

    #[tokio::test]
    async fn root_cert_path_defaults_to_etc_ssl_certs() {
        let ttd = TempTedgeDir::new();
        let mapper_dir = ttd.utf8_path().join("mappers/tb");
        tokio::fs::create_dir_all(&mapper_dir).await.unwrap();
        let tedge_config = TEdgeConfig::load_toml_str("");
        let config = make_config(None);

        let effective = resolve_effective_config(&config, &tedge_config)
            .await
            .unwrap();

        assert_eq!(
            effective.root_cert_path.value,
            Utf8PathBuf::from("/etc/ssl/certs")
        );
        assert!(matches!(
            effective.root_cert_path.source,
            ConfigSource::Default
        ));
    }
}
