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
use certificate::parse_root_certificate;
use certificate::parse_root_certificate::SecretString;
use certificate::CertificateError;
use tedge_config_macros::all_or_nothing;
use tracing::log::debug;

use super::CloudConfig;
use super::TEdgeConfigReaderDevice;

use certificate::parse_root_certificate::CryptokiConfig;
use certificate::parse_root_certificate::CryptokiConfigDirect;
use mqtt_channel::read_password;
use mqtt_channel::AuthenticationConfig;

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
pub struct TEdgeMqttClientAuthConfig {
    pub ca_dir: Option<Utf8PathBuf>,
    pub ca_file: Option<Utf8PathBuf>,
    pub client_cert: Option<MqttAuthClientCertConfig>,
    pub username: Option<String>,
    pub password_file: Option<Utf8PathBuf>,
}

#[derive(Debug, Clone, Default)]
pub struct MqttAuthClientCertConfig {
    pub cert_file: Utf8PathBuf,
    pub key_file: Utf8PathBuf,
}

impl TryFrom<TEdgeMqttClientAuthConfig> for mqtt_channel::AuthenticationConfig {
    type Error = anyhow::Error;

    fn try_from(config: TEdgeMqttClientAuthConfig) -> Result<Self, Self::Error> {
        let mut authentication_config = AuthenticationConfig::default();
        // Adds all certificates present in `ca_file` file to the trust store.
        if let Some(ca_file) = config.ca_file {
            debug!(target: "MQTT", "Using CA certificate file: {}", ca_file);
            let cert_store = &mut authentication_config.get_cert_store_mut();
            parse_root_certificate::add_certs_from_file(cert_store, ca_file)?;
        }

        // Adds all certificate from all files in the directory `ca_dir` to the trust store.
        if let Some(ca_dir) = config.ca_dir {
            debug!(target: "MQTT", "Using CA certificate directory: {}", ca_dir);
            let cert_store = &mut authentication_config.get_cert_store_mut();
            parse_root_certificate::add_certs_from_directory(cert_store, ca_dir)?;
        }

        // Provides client certificate and private key for authentication.
        if let Some(client_cert) = config.client_cert {
            debug!(target: "MQTT", "Using client certificate file: {}", client_cert.cert_file);
            debug!(target: "MQTT", "Using client private key file: {}", client_cert.key_file);
            authentication_config.set_cert_config(client_cert.cert_file, client_cert.key_file)?;
        }

        // Provides client username/password for authentication.
        if let Some(username) = config.username {
            debug!(target: "MQTT", "Using client username: {username}");
            authentication_config.set_username(username);

            // Password can be set only when username is set.
            if let Some(password_file) = config.password_file {
                debug!(target: "MQTT", "Using client password file: {}", password_file);
                if let Ok(password) = read_password(&password_file) {
                    authentication_config.set_password(password);
                }
            }
        }

        Ok(authentication_config)
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

        let mqtt_client_auth_config = self.mqtt_client_auth_config();
        mqtt_config.with_client_auth(mqtt_client_auth_config.try_into()?)?;

        Ok(mqtt_config)
    }

    /// Returns an authentication configuration for an MQTT client that will connect to the MQTT
    /// broker of a cloud given in the parameter.
    pub fn mqtt_auth_config_cloud_broker(
        &self,
        cloud: &dyn CloudConfig,
    ) -> anyhow::Result<MqttAuthConfigCloudBroker> {
        // if client cert is set, then either cryptoki or key file must be set
        let client_auth = match self.device.cryptoki_config(Some(cloud))? {
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
    pub fn mqtt_client_auth_config(&self) -> TEdgeMqttClientAuthConfig {
        let mut client_auth = TEdgeMqttClientAuthConfig {
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
            client_cert: None,
            username: self.mqtt.client.auth.username.or_none().cloned(),
            password_file: self
                .mqtt
                .client
                .auth
                .password_file
                .or_none()
                .cloned()
                .map(Utf8PathBuf::from),
        };

        // Both these options have to either be set or not set
        if let Ok(Some((client_cert, client_key))) = all_or_nothing((
            self.mqtt.client.auth.cert_file.as_ref(),
            self.mqtt.client.auth.key_file.as_ref(),
        )) {
            client_auth.client_cert = Some(MqttAuthClientCertConfig {
                cert_file: client_cert.clone().into(),
                key_file: client_key.clone().into(),
            })
        }

        client_auth
    }
}

impl TEdgeConfigReaderDevice {
    /// Returns the cryptoki configuration.
    ///
    /// - `Err` if config doesn't fit schema (e.g. set to module but module_path not set)
    /// - `Ok(None)` if mode set to `off`
    /// - `Ok(Some(CryptokiConfig))` for mode `socket` or `module`
    pub fn cryptoki_config(
        &self,
        cloud: Option<&dyn CloudConfig>,
    ) -> Result<Option<CryptokiConfig>, anyhow::Error> {
        let cryptoki = &self.cryptoki;
        let uri = cloud
            .and_then(|c| c.key_uri().or(self.key_uri.or_none().cloned()))
            .or(self.key_uri.or_none().cloned());
        let pin = cloud
            .and_then(|c| c.key_pin().or(self.key_pin.or_none().cloned()))
            .map(|p| SecretString::new(p.to_string()));

        match cryptoki.mode {
            Cryptoki::Off => Ok(None),
            Cryptoki::Module => Ok(Some(CryptokiConfig::Direct(CryptokiConfigDirect {
                module_path: cryptoki
                    .module_path
                    .or_config_not_set()
                    .context("required because `device.cryptoki.mode` is set to `module`")?
                    .clone()
                    .into(),
                pin: SecretString::new(cryptoki.pin.to_string()),
                uri,
            }))),
            Cryptoki::Socket => Ok(Some(CryptokiConfig::SocketService {
                socket_path: cryptoki.socket_path.clone(),
                uri,
                pin,
            })),
        }
    }
}
