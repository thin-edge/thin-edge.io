//! MQTT connect and auth configuration.
//!
//! thin-edge MQTT clients connect to the local broker, but sometimes we also connect to the cloud
//! broker directly. These different brokers support different authentication methods. This module
//! reads correct fields from tedge_config and provides correct rustls configuration for these
//! different clients.

use crate::models::Cryptoki;
use crate::TEdgeConfig;
use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::AuthPin;
use certificate::CertificateError;
use tedge_config_macros::all_or_nothing;

use super::CloudConfig;
use super::TEdgeConfigReaderDeviceCryptoki;

use certificate::parse_root_certificate::CryptokiConfig;
use certificate::parse_root_certificate::CryptokiConfigDirect;

/// An MQTT authentication configuration for connecting to the remote cloud broker.
#[derive(Debug, Clone, Default)]
pub struct MqttAuthConfigCloudBroker {
    pub ca_path: Utf8PathBuf,
    pub client: Option<MqttAuthClientConfigCloudBroker>,
}

/// MQTT TLS client authentication.
#[derive(Debug, Clone)]
pub struct MqttAuthClientConfigCloudBroker {
    pub cert_file: Utf8PathBuf,
    pub private_key: PrivateKeyType,
}

#[derive(Debug, Clone)]
pub enum PrivateKeyType {
    File(Utf8PathBuf),
    Cryptoki(CryptokiConfig),
}

impl MqttAuthConfigCloudBroker {
    pub fn to_rustls_client_config(self) -> anyhow::Result<rustls::ClientConfig> {
        let Some(MqttAuthClientConfigCloudBroker {
            cert_file,
            private_key,
        }) = self.client
        else {
            todo!("no client auth not supported yet");
        };

        let client_config = match private_key {
            PrivateKeyType::File(key_file) => {
                certificate::parse_root_certificate::create_tls_config(
                    self.ca_path,
                    key_file,
                    cert_file,
                )
            }
            PrivateKeyType::Cryptoki(cryptoki_config) => {
                certificate::parse_root_certificate::create_tls_config_cryptoki(
                    self.ca_path,
                    cert_file,
                    cryptoki_config,
                )
            }
        }
        .context("Failed to create TLS client config")?;
        Ok(client_config)
    }
}

/// An MQTT authentication configuration for connecting to the local broker.
///
/// If ca_dir and ca_file are both not set, then server authentication isn't used.
#[derive(Debug, Clone, Default)]
pub struct MqttAuthConfig {
    pub ca_dir: Option<Utf8PathBuf>,
    pub ca_file: Option<Utf8PathBuf>,
    pub client: Option<MqttAuthClientConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct MqttAuthClientConfig {
    pub cert_file: Utf8PathBuf,
    pub key_file: Utf8PathBuf,
}

impl MqttAuthConfig {
    pub fn to_rustls_client_config(self) -> anyhow::Result<Option<rustls::ClientConfig>> {
        let Some(ca) = self.ca_dir.or(self.ca_file) else {
            return Ok(None);
        };

        let Some(MqttAuthClientConfig {
            cert_file,
            key_file,
        }) = self.client
        else {
            let client_config =
                certificate::parse_root_certificate::create_tls_config_without_client_cert(ca)?;
            return Ok(Some(client_config));
        };

        let client_config =
            certificate::parse_root_certificate::create_tls_config(ca, key_file, cert_file)
                .context("Failed to create TLS client config")?;

        Ok(Some(client_config))
    }
}

impl TEdgeConfig {
    /// Returns a [`rustls::ClientConfig`] for an MQTT client that will connect to the MQTT broker
    /// of a cloud given in the parameter.
    pub fn mqtt_client_config_rustls(
        &self,
        cloud: &dyn CloudConfig,
    ) -> anyhow::Result<rustls::ClientConfig> {
        let client_config = self
            .mqtt_auth_config_cloud_broker(cloud)?
            .to_rustls_client_config()?;

        Ok(client_config)
    }

    pub fn mqtt_config(&self) -> Result<mqtt_channel::Config, CertificateError> {
        let host = self.mqtt.client.host.as_str();
        let port = u16::from(self.mqtt.client.port);

        let mut mqtt_config = mqtt_channel::Config::default()
            .with_host(host)
            .with_port(port);

        // If these options are not set, just don't use them
        // Configure certificate authentication
        if let Some(ca_file) = self.mqtt.client.auth.ca_file.or_none() {
            mqtt_config.with_cafile(ca_file)?;
        }
        if let Some(ca_path) = self.mqtt.client.auth.ca_dir.or_none() {
            mqtt_config.with_cadir(ca_path)?;
        }

        // Both these options have to either be set or not set, so we keep
        // original error to rethrow when only one is set
        if let Ok(Some((client_cert, client_key))) = all_or_nothing((
            self.mqtt.client.auth.cert_file.as_ref(),
            self.mqtt.client.auth.key_file.as_ref(),
        )) {
            mqtt_config.with_client_auth(client_cert, client_key)?;
        }

        Ok(mqtt_config)
    }

    /// Returns an authentication configuration for an MQTT client that will connect to the MQTT
    /// broker of a cloud given in the parameter.
    pub fn mqtt_auth_config_cloud_broker(
        &self,
        cloud: &dyn CloudConfig,
    ) -> anyhow::Result<MqttAuthConfigCloudBroker> {
        // if client cert is set, then either cryptoki or key file must be set
        let client_auth = match self.device.cryptoki.config()? {
            Some(cryptoki_config) => MqttAuthClientConfigCloudBroker {
                cert_file: cloud.device_cert_path().to_path_buf(),
                private_key: PrivateKeyType::Cryptoki(cryptoki_config),
            },
            None => MqttAuthClientConfigCloudBroker {
                cert_file: cloud.device_cert_path().to_path_buf(),
                private_key: PrivateKeyType::File(cloud.device_key_path().to_path_buf()),
            },
        };

        Ok(MqttAuthConfigCloudBroker {
            ca_path: cloud.root_cert_path().to_path_buf(),
            client: Some(client_auth),
        })
    }

    /// Returns an authentication configuration for an MQTT client that will connect to the local MQTT broker.
    pub fn mqtt_client_auth_config(&self) -> MqttAuthConfig {
        let mut client_auth = MqttAuthConfig {
            ca_dir: self
                .mqtt
                .client
                .auth
                .ca_dir
                .or_none()
                .cloned()
                .map(Utf8PathBuf::from),
            ca_file: self
                .mqtt
                .client
                .auth
                .ca_file
                .or_none()
                .cloned()
                .map(Utf8PathBuf::from),
            client: None,
        };
        // Both these options have to either be set or not set
        if let Ok(Some((client_cert, client_key))) = all_or_nothing((
            self.mqtt.client.auth.cert_file.as_ref(),
            self.mqtt.client.auth.key_file.as_ref(),
        )) {
            client_auth.client = Some(MqttAuthClientConfig {
                cert_file: client_cert.clone().into(),
                key_file: client_key.clone().into(),
            })
        }
        client_auth
    }
}

impl TEdgeConfigReaderDeviceCryptoki {
    pub fn config(&self) -> Result<Option<CryptokiConfig>, anyhow::Error> {
        match self.mode {
            Cryptoki::Off => Ok(None),
            Cryptoki::Module => Ok(Some(CryptokiConfig::Direct(CryptokiConfigDirect {
                module_path: self
                    .module_path
                    .or_config_not_set()
                    .context("required because `device.cryptoki.mode` is set to `module`")?
                    .clone()
                    .into(),
                pin: AuthPin::new(self.pin.to_string()),
                serial: None,
                uri: None,
            }))),
            Cryptoki::Socket => Ok(Some(CryptokiConfig::SocketService {
                socket_path: self.socket_path.clone(),
            })),
        }
    }
}
