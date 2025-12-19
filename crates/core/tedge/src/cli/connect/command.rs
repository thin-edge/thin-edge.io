#[cfg(feature = "aws")]
use crate::bridge::aws::BridgeConfigAwsParams;
#[cfg(feature = "azure")]
use crate::bridge::azure::BridgeConfigAzureParams;
#[cfg(feature = "c8y")]
use crate::bridge::c8y::BridgeConfigC8yParams;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::cli::common::Cloud;
use crate::cli::common::MaybeBorrowedCloud;
#[cfg(feature = "c8y")]
use crate::cli::connect::c8y::*;
use crate::cli::connect::*;
use crate::cli::log::ConfigLogger;
use crate::cli::log::Fancy;
use crate::cli::log::Spinner;
use crate::cli::CertificateShift;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::system_services::*;
use crate::warning;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::bail;
#[cfg(feature = "c8y")]
use c8y_api::http_proxy::read_c8y_credentials;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::CryptokiConfig;
use mqtt_channel::Topic;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::service_health_topic;
use tedge_config;
use tedge_config::all_or_nothing;
use tedge_config::models::auth_method::AuthType;
use tedge_config::models::proxy_scheme::ProxyScheme;
use tedge_config::models::AbsolutePath;
#[cfg(any(feature = "aws", feature = "azure"))]
use tedge_config::models::HostPort;
use tedge_config::models::TopicPrefix;
use tedge_config::models::MQTT_SERVICE_TLS_PORT;
#[cfg(feature = "aws")]
use tedge_config::tedge_toml::mapper_config::AwsMapperSpecificConfig;
#[cfg(feature = "azure")]
use tedge_config::tedge_toml::mapper_config::AzMapperSpecificConfig;
#[cfg(feature = "c8y")]
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::mapper_config::HasUrl;
use tedge_config::tedge_toml::mapper_config::MapperConfig;
use tedge_config::tedge_toml::mapper_config::SpecialisedCloudConfig;
#[cfg(feature = "c8y")]
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::TEdgeConfigReaderMqtt;
use tedge_config::TEdgeConfig;
#[cfg(any(feature = "aws", feature = "azure"))]
use tedge_config::TEdgeConfigError;
use tedge_utils::file::path_exists;
use tedge_utils::paths::create_directories;
use tedge_utils::paths::ok_if_not_found;
use tedge_utils::paths::DraftFile;
use tracing::warn;
use yansi::Paint as _;

pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(feature = "c8y")]
pub(crate) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 20;
#[cfg(any(feature = "aws", feature = "azure"))]
const MQTT_TLS_PORT: u16 = 8883;

pub struct ConnectCommand {
    pub cloud: Cloud,
    pub is_test_connection: bool,
    pub offline_mode: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
    pub is_reconnect: bool,
}

pub enum DeviceStatus {
    AlreadyExists,
    Unknown,
}

#[async_trait::async_trait]
impl Command for ConnectCommand {
    fn description(&self) -> String {
        if self.is_test_connection {
            format!("test connection to {} cloud.", self.cloud)
        } else if self.is_reconnect {
            format!("reconnect to {} cloud.", self.cloud)
        } else {
            format!("connect to {} cloud.", self.cloud)
        }
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let bridge_config = bridge_config(&tedge_config, &self.cloud)
            .await
            .map_err(anyhow::Error::new)?;
        let credentials_path = credentials_path_for(&tedge_config, &self.cloud).await?;

        let cloud = tedge_config
            .as_cloud_config((&self.cloud).into())
            .map_err(anyhow::Error::new)?;

        let cryptoki_key_uri =
            tedge_config
                .device
                .cryptoki_config(Some(&*cloud))?
                .map(|c| match c {
                    CryptokiConfig::Direct(d) => d.uri,
                    CryptokiConfig::SocketService { uri, .. } => uri,
                });
        let cryptoki_mode = tedge_config.device.cryptoki.mode.clone();
        let cryptoki_status = match cryptoki_key_uri {
            None => "off".to_string(),
            Some(None) => format!("{cryptoki_mode}"),
            Some(Some(key)) => format!("{cryptoki_mode} (key: {key})"),
        };

        let log_msg = if self.is_test_connection {
            format!("Testing connection to {}", self.cloud)
        } else if self.is_reconnect {
            format!("Reconnecting to {}", self.cloud)
        } else {
            format!("Connecting to {}", self.cloud)
        };

        ConfigLogger::log(
            log_msg,
            &bridge_config,
            &*self.service_manager,
            &self.cloud,
            credentials_path.as_ref(),
            &cryptoki_status,
            tedge_config.proxy.address.or_none(),
            tedge_config.proxy.username.or_none().map(|u| u.as_str()),
            cloud.mapper_config_location(),
        );

        validate_config(&tedge_config, &self.cloud)?;

        if self.is_test_connection {
            self.check_bridge(&tedge_config, &bridge_config)
                .await
                .map_err(<_>::into)
        } else {
            fail_if_already_connected(&tedge_config, &bridge_config).map_err(anyhow::Error::new)?;

            let shift_failed = match bridge_config.certificate_awaits_validation().await {
                None => false,
                Some(certificate_shift) => {
                    let shift_done = self
                        .validate_new_certificate(&tedge_config, &bridge_config, certificate_shift)
                        .await
                        .unwrap_or(false);
                    !shift_done
                }
            };

            let connected = self.connect_bridge(&tedge_config, &bridge_config).await;
            if connected.is_ok() && shift_failed {
                eprintln!("Successfully connected, however not using the new certificate");
                std::process::exit(3);
            }
            connected.map_err(<_>::into)
        }
    }
}

impl ConnectCommand {
    async fn check_bridge(
        &self,
        tedge_config: &TEdgeConfig,
        bridge_config: &BridgeConfig,
    ) -> Result<(), Fancy<ConnectError>> {
        // If the bridge is part of the mapper, the bridge config file won't exist
        // TODO tidy me up once mosquitto is no longer required for bridge
        if self
            .check_if_bridge_exists(tedge_config, bridge_config)
            .await
        {
            match self.check_connection(tedge_config).await {
                Ok(DeviceStatus::AlreadyExists) => {
                    match self
                        .tenant_matches_configured_url(tedge_config, bridge_config)
                        .await?
                    {
                        // Check failed, warning has been printed already
                        // Don't tell them the connection test succeeded because that's not true
                        Some(false) => {}
                        // Either the check succeeded or it wasn't relevant (e.g. non-Cumulocity connection)
                        Some(true) | None => {
                            eprintln!(
                                "Connection check to {} cloud is successful.",
                                bridge_config.cloud_name
                            )
                        }
                    }

                    Ok(())
                }
                Ok(DeviceStatus::Unknown) => Err(ConnectError::UnknownDeviceStatus.into()),
                Err(err) => Err(err),
            }
        } else {
            Err((ConnectError::DeviceNotConnected {
                cloud: self.cloud.to_string(),
            })
            .into())
        }
    }

    async fn connect_bridge(
        &self,
        tedge_config: &TEdgeConfig,
        bridge_config: &BridgeConfig,
    ) -> Result<(), Fancy<ConnectError>> {
        let updated_mosquitto_config = CommonMosquittoConfig::from_tedge_config(tedge_config);

        match self
            .new_bridge(tedge_config, bridge_config, &updated_mosquitto_config)
            .await
        {
            Ok(()) => (),
            Err(Fancy {
                err:
                    ConnectError::SystemServiceError(SystemServiceError::ServiceManagerUnavailable {
                        ..
                    }),
                ..
            }) => return Ok(()),
            Err(err) => return Err(err),
        }

        if bridge_config.use_mapper && bridge_config.bridge_location == BridgeLocation::BuiltIn {
            // If the bridge is built in, the mapper needs to be running with the new configuration
            // to be connected
            self.start_mapper().await;
        }

        let mut connection_check_success = true;
        if !self.offline_mode {
            match self
                .check_connection_with_retries(
                    tedge_config,
                    bridge_config.connection_check_attempts,
                )
                .await
            {
                Ok(DeviceStatus::AlreadyExists) => {}
                _ => {
                    warning!(
                        "Bridge has been configured, but {} connection check failed.",
                        self.cloud
                    );
                    connection_check_success = false;
                }
            }
        }

        if bridge_config.use_mapper && bridge_config.bridge_location == BridgeLocation::Mosquitto {
            // If the bridge is in mosquitto, the mapper should only start once the cloud connection
            // is verified
            self.start_mapper().await;
        }

        match &self.cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(_) => {
                if connection_check_success {
                    let _ = self
                        .tenant_matches_configured_url(tedge_config, bridge_config)
                        .await;
                }
                enable_software_management(bridge_config, &*self.service_manager).await;
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(_) => (),
            #[cfg(feature = "azure")]
            Cloud::Azure(_) => (),
        }

        if connection_check_success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Connection check failed").into())
        }
    }

    /// Validate that the new certificate is actually accepted by the cloud endpoint
    ///
    /// Return:
    /// - Ok(true) when the new certificate has been promoted as the current one
    /// - Ok(false) when it's safer to keep the current certificate untouched
    /// - Err(err) when the endpoint is not correctly configured
    async fn validate_new_certificate(
        &self,
        tedge_config: &TEdgeConfig,
        bridge_config: &BridgeConfig,
        certificate_shift: CertificateShift,
    ) -> Result<bool, ConfigError> {
        if self.offline_mode {
            eprintln!("Offline mode. Skipping new device certificate validation");
            eprintln!(
                "  => use current certificate {}",
                &certificate_shift.active_cert_path
            );
            eprintln!(
                "  => ignoring new certificate {}",
                &certificate_shift.new_cert_path
            );
            return Ok(false);
        }

        let mut attempt = 0;
        let max_attempts = bridge_config.connection_check_attempts;
        let res = loop {
            attempt += 1;
            let banner = if attempt == 1 {
                format!(
                    "Validating new certificate: {}",
                    &certificate_shift.new_cert_path
                )
            } else {
                format!("Validating new certificate: attempt {attempt} of {max_attempts}")
            };

            let spinner = Spinner::start(banner);
            let res = self
                .connect_with_new_certificate(tedge_config, bridge_config, &certificate_shift)
                .await;
            match spinner.finish(res) {
                Ok(()) => break Ok(()),
                Err(err) if attempt >= max_attempts => break Err(err),
                Err(_) => tokio::time::sleep(Duration::from_secs(2)).await,
            }
        };

        if let Err(err) = res {
            eprintln!("Error validating the new certificate: {err}");
            eprintln!(
                "  => keep using the current certificate unchanged {}",
                &certificate_shift.active_cert_path
            );
            return Ok(false);
        }

        if let Err(err) = certificate_shift.promote_new_certificate().await {
            eprintln!("Error replacing the device certificate by the new one: {err}");
            eprintln!(
                "  => keep using the current certificate unchanged {}",
                certificate_shift.active_cert_path
            );
            return Ok(false);
        }

        eprintln!(
            "The new certificate is now the active certificate {}",
            certificate_shift.active_cert_path
        );
        Ok(true)
    }

    async fn connect_with_new_certificate(
        &self,
        tedge_config: &TEdgeConfig,
        _bridge_config: &BridgeConfig,
        _certificate_shift: &CertificateShift,
    ) -> anyhow::Result<()> {
        match &self.cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(profile_name) => {
                let device_type = &tedge_config.device.ty;
                let c8y_config =
                    tedge_config.mapper_config::<C8yMapperSpecificConfig>(profile_name)?;
                let mut mqtt_auth_config =
                    tedge_config.mqtt_auth_config_cloud_broker(&c8y_config)?;
                if let Some(client_config) = mqtt_auth_config.client.as_mut() {
                    _certificate_shift
                        .new_cert_path
                        .clone_into(&mut client_config.cert_file)
                }

                create_device_with_direct_connection(_bridge_config, device_type, mqtt_auth_config)
                    .await
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(_) => Ok(()),
            #[cfg(feature = "azure")]
            Cloud::Azure(_) => Ok(()),
        }
    }
}

async fn credentials_path_for(
    _config: &TEdgeConfig,
    cloud: &Cloud,
) -> anyhow::Result<Option<AbsolutePath>> {
    match cloud {
        #[cfg(feature = "c8y")]
        Cloud::C8y(profile) => {
            let c8y_config = _config.mapper_config::<C8yMapperSpecificConfig>(profile)?;
            Ok(Some(c8y_config.cloud_specific.credentials_path.clone()))
        }
        #[cfg(feature = "aws")]
        Cloud::Aws(_) => Ok(None),
        #[cfg(feature = "azure")]
        Cloud::Azure(_) => Ok(None),
    }
}

impl ConnectCommand {
    async fn tenant_matches_configured_url(
        &self,
        tedge_config: &TEdgeConfig,
        bridge_config: &BridgeConfig,
    ) -> Result<Option<bool>, Fancy<ConnectError>> {
        match &self.cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(profile) => {
                let c8y_config = tedge_config.mapper_config::<C8yMapperSpecificConfig>(profile)?;

                if bridge_config.auth_type == AuthType::Certificate && !self.offline_mode {
                    tenant_matches_configured_url(
                        tedge_config,
                        profile.as_deref(),
                        &c8y_config
                            .cloud_specific
                            .mqtt
                            .or_none()
                            .map(|u| u.host().to_string())
                            .unwrap_or_default(),
                        &c8y_config
                            .cloud_specific
                            .http
                            .or_none()
                            .map(|u| u.host().to_string())
                            .unwrap_or_default(),
                    )
                    .await
                    .map(Some)
                } else {
                    Ok(None)
                }
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(_) => Ok(None),
            #[cfg(feature = "azure")]
            Cloud::Azure(_) => Ok(None),
        }
    }

    async fn check_connection_with_retries(
        &self,
        tedge_config: &TEdgeConfig,
        max_attempts: u32,
    ) -> Result<DeviceStatus, Fancy<ConnectError>> {
        for i in 1..max_attempts {
            let result = self.check_connection(tedge_config).await;
            if let Ok(DeviceStatus::AlreadyExists) = result {
                return result;
            }
            eprintln!(
                "Connection test failed, attempt {} of {}\n",
                i, max_attempts,
            );
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        self.check_connection(tedge_config).await
    }

    async fn check_connection(
        &self,
        tedge_config: &TEdgeConfig,
    ) -> Result<DeviceStatus, Fancy<ConnectError>> {
        let spinner = Spinner::start("Verifying device is connected to cloud");
        let res = match &self.cloud {
            #[cfg(feature = "azure")]
            Cloud::Azure(profile) => {
                azure::check_device_status_azure(tedge_config, profile.as_deref()).await
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(profile) => {
                aws::check_device_status_aws(tedge_config, profile.as_deref()).await
            }
            #[cfg(feature = "c8y")]
            Cloud::C8y(profile) => check_device_status_c8y(tedge_config, profile.as_deref()).await,
        };
        spinner.finish(res)
    }

    async fn check_if_bridge_exists(
        &self,
        tedge_config: &TEdgeConfig,
        br_config: &BridgeConfig,
    ) -> bool {
        let bridge_conf_path = tedge_config
            .root_dir()
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&*br_config.config_file);

        path_exists(&bridge_conf_path).await
    }

    async fn start_mapper(&self) {
        if which_async("tedge-mapper").await.is_err() {
            warning!("tedge-mapper is not installed.");
        } else {
            let spinner = Spinner::start(format!("Enabling {}", self.cloud.mapper_service()));
            let _ = spinner.finish(
                start_and_enable_service(&*self.service_manager, self.cloud.mapper_service()).await,
            );
        }
    }
}

fn validate_config(config: &TEdgeConfig, cloud: &MaybeBorrowedCloud<'_>) -> anyhow::Result<()> {
    if !config.mqtt.bridge.built_in && config.proxy.address.or_none().is_some() {
        warn!("`proxy.address` is configured without the built-in bridge enabled. The bridge MQTT connection to the cloud will {} communicate via the configured proxy.", "not".bold())
    }
    match cloud {
        #[cfg(feature = "aws")]
        MaybeBorrowedCloud::Aws(_) => {
            let configs = config.all_mapper_configs::<AwsMapperSpecificConfig>();
            disallow_matching_url_device_id(&configs)?;
            disallow_matching_bridge_topic_prefix(&configs)?;
        }
        #[cfg(feature = "azure")]
        MaybeBorrowedCloud::Azure(_) => {
            let configs = config.all_mapper_configs::<AzMapperSpecificConfig>();
            disallow_matching_url_device_id(&configs)?;
            disallow_matching_bridge_topic_prefix(&configs)?;
        }
        #[cfg(feature = "c8y")]
        MaybeBorrowedCloud::C8y(_) => {
            let configs = config.all_mapper_configs::<C8yMapperSpecificConfig>();
            disallow_matching_url_device_id(&configs)?;
            disallow_matching_bridge_topic_prefix(&configs)?;
            disallow_matching_proxy_bind_port(&configs)?;
        }
    }
    Ok(())
}

type MapperConfigData<T> = (MapperConfig<T>, Option<ProfileName>);

fn disallow_matching_url_device_id<T>(mapper_configs: &[MapperConfigData<T>]) -> anyhow::Result<()>
where
    MapperConfig<T>: HasUrl,
    T: SpecialisedCloudConfig,
{
    let url_entries = mapper_configs.iter().map(|(config, profile)| {
        let value = config.configured_url();
        (
            (profile, value.key()),
            value.or_none().map(|h| h.to_string()),
        )
    });

    for url_matches in find_all_matching(url_entries) {
        let device_id_entries = mapper_configs.iter().filter_map(|(config, profile)| {
            let (_, url_key) = url_matches.iter().find(|(p, _)| *p == profile)?;
            let value = config.device.id().ok();
            Some(((profile, config.device.id_key(), *url_key), value))
        });
        if let Some(matches) = find_matching(device_id_entries) {
            let url_keys: String = matches
                .iter()
                .map(|(_, _key, url_key)| format!("{}", url_key.yellow().bold()))
                .collect::<Vec<_>>()
                .join(", ");
            let device_id_keys: String = matches
                .iter()
                .map(|(_, key, _)| format!("{}", key.yellow().bold()))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "You have matching URLs and device IDs for different profiles.

{url_keys} are set to the same value, but so are {device_id_keys}.

Each cloud profile requires either a unique URL or unique device ID, \
so it corresponds to a unique device in the associated cloud."
            );
        }
    }
    Ok(())
}

#[cfg(feature = "c8y")]
fn disallow_matching_bridge_topic_prefix<T>(configs: &[MapperConfigData<T>]) -> anyhow::Result<()>
where
    T: SpecialisedCloudConfig,
{
    let entries = configs.iter().map(|(config, _profile)| {
        let value = &config.bridge.topic_prefix;
        (value.key().clone(), (*value).clone())
    });
    if let Some(matches) = find_matching(entries) {
        let keys: String = matches
            .iter()
            .map(|k| format!("{}", k.yellow().bold()))
            .collect::<Vec<_>>()
            .join(", ");

        bail!("The configurations: {keys} should be set to different values before connecting, but are currently set to the same value");
    }
    Ok(())
}

#[cfg(feature = "c8y")]
fn disallow_matching_proxy_bind_port(
    configs: &[MapperConfigData<C8yMapperSpecificConfig>],
) -> anyhow::Result<()> {
    let entries = configs.iter().map(|(config, _profile)| {
        let value = &config.cloud_specific.proxy.bind.port;
        (value.key().clone(), **value)
    });
    if let Some(matches) = find_matching(entries) {
        let keys: String = matches
            .iter()
            .map(|k| format!("{}", k.yellow().bold()))
            .collect::<Vec<_>>()
            .join(", ");

        bail!("The configurations: {keys} should be set to different values before connecting, but are currently set to the same value");
    }
    Ok(())
}

fn find_matching<K, V: Hash + Eq>(entries: impl Iterator<Item = (K, V)>) -> Option<Vec<K>> {
    let match_map = entries.fold(HashMap::<V, Vec<K>>::new(), |mut acc, (key, value)| {
        acc.entry(value).or_default().push(key);
        acc
    });

    match_map.into_values().find(|t| t.len() > 1)
}

fn find_all_matching<K, V: Hash + Eq>(entries: impl Iterator<Item = (K, V)>) -> Vec<Vec<K>> {
    let match_map = entries.fold(HashMap::<V, Vec<K>>::new(), |mut acc, (key, value)| {
        acc.entry(value).or_default().push(key);
        acc
    });

    match_map.into_values().filter(|t| t.len() > 1).collect()
}

pub async fn bridge_config(
    config: &TEdgeConfig,
    cloud: &MaybeBorrowedCloud<'_>,
) -> Result<BridgeConfig, ConfigError> {
    let bridge_location = match config.mqtt.bridge.built_in {
        true => BridgeLocation::BuiltIn,
        false => BridgeLocation::Mosquitto,
    };
    let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());
    let proxy = config
        .proxy
        .address
        .or_none()
        .map(|address| {
            let rustls_config = config.cloud_client_tls_config();
            Ok::<_, ConfigError>(rumqttc::Proxy {
                ty: match address.scheme() {
                    ProxyScheme::Http => rumqttc::ProxyType::Http,
                    ProxyScheme::Https => rumqttc::ProxyType::Https(
                        rumqttc::TlsConfiguration::Rustls(Arc::new(rustls_config)),
                    ),
                },
                addr: address.host().to_string(),
                port: address.port().into(),
                auth: match all_or_nothing((
                    config.proxy.username.clone(),
                    config.proxy.password.clone(),
                ))
                .map_err(|e| anyhow::anyhow!(e))?
                {
                    Some((username, password)) => rumqttc::ProxyAuth::Basic { username, password },
                    None => rumqttc::ProxyAuth::None,
                },
            })
        })
        .transpose()?;

    match cloud {
        #[cfg(feature = "azure")]
        MaybeBorrowedCloud::Azure(profile) => {
            let az_config = config.mapper_config::<AzMapperSpecificConfig>(profile)?;

            let params = BridgeConfigAzureParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    az_config.url().or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: az_config.root_cert_path.clone().into(),
                remote_clientid: az_config.device.id()?.clone(),
                bridge_certfile: az_config.device.cert_path.clone().into(),
                bridge_keyfile: az_config.device.key_path.clone().into(),
                bridge_location,
                topic_prefix: az_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: az_config.bridge.keepalive_interval.duration(),
                proxy,
            };

            Ok(BridgeConfig::from(params))
        }
        #[cfg(feature = "aws")]
        MaybeBorrowedCloud::Aws(profile) => {
            let aws_config = config.mapper_config::<AwsMapperSpecificConfig>(profile)?;

            let params = BridgeConfigAwsParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    aws_config.url().or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: aws_config.root_cert_path.clone().into(),
                remote_clientid: aws_config.device.id()?.clone(),
                bridge_certfile: aws_config.device.cert_path.clone().into(),
                bridge_keyfile: aws_config.device.key_path.clone().into(),
                bridge_location,
                topic_prefix: aws_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: aws_config.bridge.keepalive_interval.duration(),
                proxy,
            };

            Ok(BridgeConfig::from(params))
        }
        #[cfg(feature = "c8y")]
        MaybeBorrowedCloud::C8y(profile) => {
            use tedge_config::models::MQTT_CORE_TLS_PORT;
            let c8y_config = config.mapper_config::<C8yMapperSpecificConfig>(profile)?;

            let (remote_username, remote_password) = match c8y_config
                .cloud_specific
                .auth_method
                .to_type(&c8y_config.cloud_specific.credentials_path)
            {
                AuthType::Certificate => (None, None),
                AuthType::Basic => {
                    let (username, password) =
                        read_c8y_credentials(&c8y_config.cloud_specific.credentials_path)?;
                    (Some(username), Some(password))
                }
            };

            let use_mqtt_service = c8y_config.cloud_specific.mqtt_service.enabled;
            let mut mqtt_host = c8y_config.cloud_specific.mqtt.or_config_not_set()?.clone();

            // If the MQTT URL is still pointing to the default core MQTT port, when MQTT service is enabled
            // implicitly switch to the MQTT service port.
            // When the port is not the default one,
            // we assume that the user has explicitly configured it to point to the MQTT service endpoint.
            if use_mqtt_service && mqtt_host.port().0 == MQTT_CORE_TLS_PORT {
                mqtt_host.set_port(MQTT_SERVICE_TLS_PORT);
            }

            let params = BridgeConfigC8yParams {
                mqtt_host,
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: c8y_config.root_cert_path.clone().into(),
                remote_clientid: c8y_config.device.id()?.clone(),
                remote_username,
                remote_password,
                bridge_certfile: c8y_config.device.cert_path.clone().into(),
                bridge_keyfile: c8y_config.device.key_path.clone().into(),
                smartrest_templates: c8y_config.cloud_specific.smartrest.templates.clone(),
                smartrest_one_templates: c8y_config.cloud_specific.smartrest1.templates.clone(),
                include_local_clean_session: c8y_config
                    .cloud_specific
                    .bridge
                    .include
                    .local_cleansession,
                bridge_location,
                topic_prefix: c8y_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: c8y_config.bridge.keepalive_interval.duration(),
                proxy,
                use_mqtt_service,
                custom_topics: c8y_config.cloud_specific.mqtt_service.topics.clone(),
            };

            Ok(BridgeConfig::from(params))
        }
    }
}

pub(crate) fn bridge_health_topic(prefix: &TopicPrefix, tedge_config: &TEdgeConfig) -> Topic {
    let bridge_name = if tedge_config.mqtt.bridge.built_in {
        format!("tedge-mapper-bridge-{prefix}")
    } else {
        format!("mosquitto-{prefix}-bridge")
    };

    let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
    let device_topic_id = tedge_config.mqtt.device_topic_id.clone();
    service_health_topic(&mqtt_schema, &device_topic_id, &bridge_name)
}

#[cfg(any(feature = "aws", feature = "c8y"))]
pub(crate) fn is_bridge_health_up_message(
    message: &rumqttc::Publish,
    health_topic: &str,
    built_in_bridge: bool,
) -> bool {
    message.topic == health_topic
        && std::str::from_utf8(&message.payload).is_ok_and(|msg| {
            if built_in_bridge {
                msg.contains("\"up\"")
            } else {
                msg.contains("1")
            }
        })
}

impl ConnectCommand {
    async fn new_bridge(
        &self,
        tedge_config: &TEdgeConfig,
        bridge_config: &BridgeConfig,
        common_mosquitto_config: &CommonMosquittoConfig,
    ) -> Result<(), Fancy<ConnectError>> {
        let service_manager = &self.service_manager;
        let service_manager_result = service_manager.check_operational().await;

        if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
            &service_manager_result
        {
            warning!("'{name}' service manager is not available on the system.",);
        }

        match &self.cloud {
            #[cfg(feature = "c8y")]
            Cloud::C8y(profile_name) => {
                if self.offline_mode {
                    eprintln!("Offline mode. Skipping device creation in Cumulocity cloud.")
                } else {
                    let c8y_config =
                        tedge_config.mapper_config::<C8yMapperSpecificConfig>(profile_name)?;
                    let mqtt_auth_config =
                        tedge_config.mqtt_auth_config_cloud_broker(&c8y_config)?;
                    let spinner = Spinner::start("Creating device in Cumulocity cloud");
                    let res = create_device_with_direct_connection(
                        bridge_config,
                        &tedge_config.device.ty,
                        mqtt_auth_config,
                    )
                    .await;
                    spinner.finish(res)?;
                }
            }
            #[cfg(feature = "aws")]
            Cloud::Aws(_) => (),
            #[cfg(feature = "azure")]
            Cloud::Azure(_) => (),
        }

        if let Err(err) =
            write_generic_mosquitto_config_to_file(tedge_config, common_mosquitto_config).await
        {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = clean_up(tedge_config, bridge_config);
            return Err(err.into());
        }

        if bridge_config.bridge_location == BridgeLocation::Mosquitto {
            let spinner = Spinner::start("Creating mosquitto bridge");
            let res = write_mosquitto_bridge_config_file(tedge_config, bridge_config).await;
            spinner.finish(res)?;
        } else {
            use_built_in_bridge(tedge_config, bridge_config).await?;
        }

        if let Err(err) = service_manager_result {
            println!("'tedge connect' configured the necessary tedge components, but you will have to start the required services on your own.");
            println!("Start/restart mosquitto and other thin edge components.");
            println!("thin-edge.io works seamlessly with 'systemd'.\n");
            return Err(err.into());
        }

        restart_mosquitto(bridge_config, service_manager.as_ref(), tedge_config).await?;

        let spinner = Spinner::start("Waiting for mosquitto to be listening for connections");
        spinner.finish(wait_for_mosquitto_listening(&tedge_config.mqtt).await)?;

        if let Err(err) = service_manager
            .enable_service(SystemService::Mosquitto)
            .await
        {
            clean_up(tedge_config, bridge_config)?;
            return Err(err.into());
        }

        Ok(())
    }
}

pub async fn chown_certificate_and_key(bridge_config: &BridgeConfig) {
    // Skip chown when using Basic Auth
    if bridge_config.auth_type == AuthType::Basic {
        return;
    }

    let (user, group) = match bridge_config.bridge_location {
        BridgeLocation::BuiltIn => ("tedge", "tedge"),
        BridgeLocation::Mosquitto => (crate::BROKER_USER, crate::BROKER_GROUP),
    };
    // Ignore errors - This was the behavior with the now deprecated user manager.
    // - When `tedge cert create` is not run as root, a certificate is created but owned by the user running the command.
    // - A better approach could be to remove this `chown` and run the command as mosquitto.
    let path = &bridge_config.bridge_certfile;
    if let Err(err) =
        tedge_utils::file::change_user_and_group(path.into(), user.to_owned(), group.to_owned())
            .await
    {
        warn!("Failed to change ownership of {path} to {user}:{group}: {err}");
    }

    let path = &bridge_config.bridge_keyfile;
    // if not using a private key (e.g. because we're signing with an HSM) don't chown it
    if !path.exists() {
        return;
    }
    if let Err(err) =
        tedge_utils::file::change_user_and_group(path.into(), user.to_owned(), group.to_owned())
            .await
    {
        warn!("Failed to change ownership of {path} to {user}:{group}: {err}");
    }
}

async fn restart_mosquitto(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
    config: &TEdgeConfig,
) -> Result<(), Fancy<ConnectError>> {
    let spinner = Spinner::start("Restarting mosquitto");
    spinner
        .finish(restart_mosquitto_inner(bridge_config, service_manager).await)
        .inspect_err(|_| {
            // We want to preserve existing errors and therefore discard result of this function.
            let _ = clean_up(config, bridge_config);
        })
}
async fn restart_mosquitto_inner(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) -> Result<(), ConnectError> {
    service_manager
        .stop_service(SystemService::Mosquitto)
        .await?;
    chown_certificate_and_key(bridge_config).await;
    service_manager
        .restart_service(SystemService::Mosquitto)
        .await?;

    Ok(())
}

async fn wait_for_mosquitto_listening(mqtt: &TEdgeConfigReaderMqtt) -> Result<(), anyhow::Error> {
    let addr = format!("{}:{}", mqtt.client.host, mqtt.client.port);
    if let Some(addr) = addr.to_socket_addrs().ok().and_then(|mut o| o.next()) {
        let timeout = Duration::from_secs(MOSQUITTO_RESTART_TIMEOUT_SECONDS);
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr)).await {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!(
                "Timed out after {timeout:?} waiting for mosquitto to be listening"
            )),
        }
    } else {
        bail!("Couldn't resolve configured mosquitto address");
    }
}

#[cfg(feature = "c8y")]
async fn enable_software_management(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) {
    if bridge_config.use_agent {
        if which_async("tedge-agent").await.is_ok() {
            let spinner = Spinner::start("Enabling tedge-agent");
            let _ = spinner.finish(
                start_and_enable_service(service_manager, SystemService::TEdgeSMAgent).await,
            );
        } else {
            println!("Info: Software management is not installed. So, skipping enabling related components.\n");
        }
    }
}

async fn which_async(name: &'static str) -> Result<std::path::PathBuf, which::Error> {
    tokio::task::spawn_blocking(move || ::which::which(name))
        .await
        .unwrap()
}

async fn start_and_enable_service(
    service_manager: &dyn SystemServiceManager,
    service: SystemService<'_>,
) -> anyhow::Result<()> {
    service_manager.start_service(service).await?;
    service_manager.enable_service(service).await?;
    Ok(())
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
pub fn clean_up(
    config: &TEdgeConfig,
    bridge_config: &BridgeConfig,
) -> Result<(), Fancy<ConnectError>> {
    let path = get_bridge_config_file_path(config, bridge_config);
    Spinner::start(format!("Cleaning up {path} due to failure"))
        .finish(std::fs::remove_file(path).or_else(ok_if_not_found))?;
    Ok(())
}

pub async fn use_built_in_bridge(
    config: &TEdgeConfig,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config, bridge_config);
    tokio::fs::write(
        path,
        "# This file is left empty as the built-in bridge is enabled",
    )
    .await
    .or_else(ok_if_not_found)?;
    Ok(())
}

fn fail_if_already_connected(
    config: &TEdgeConfig,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config, bridge_config);
    if path.exists() {
        return Err(ConnectError::ConfigurationExists {
            cloud: bridge_config.cloud_name.to_string(),
        });
    }
    Ok(())
}

async fn write_generic_mosquitto_config_to_file(
    tedge_config: &TEdgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Result<(), ConnectError> {
    let dir_path = tedge_config.root_dir().join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let common_config_path =
        get_common_mosquitto_config_file_path(tedge_config, common_mosquitto_config);
    let mut common_draft = DraftFile::new(common_config_path).await?.with_mode(0o644);
    common_mosquitto_config.serialize(&mut common_draft).await?;
    common_draft.persist().await?;

    Ok(())
}

async fn write_mosquitto_bridge_config_file(
    tedge_config: &TEdgeConfig,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    if let Err(err) = write_bridge_config_to_file(tedge_config, bridge_config).await {
        // We want to preserve previous errors and therefore discard result of this function.
        let _ = clean_up(tedge_config, bridge_config);
        return Err(err);
    }

    Ok(())
}

async fn write_bridge_config_to_file(
    config: &TEdgeConfig,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let dir_path = config.root_dir().join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let config_path = get_bridge_config_file_path(config, bridge_config);
    let mut config_draft = DraftFile::new(config_path).await?.with_mode(0o644);
    bridge_config.serialize(&mut config_draft).await?;
    config_draft.persist().await?;

    Ok(())
}

fn get_bridge_config_file_path(config: &TEdgeConfig, bridge_config: &BridgeConfig) -> Utf8PathBuf {
    config
        .root_dir()
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&*bridge_config.config_file)
}

fn get_common_mosquitto_config_file_path(
    config: &TEdgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Utf8PathBuf {
    config
        .root_dir()
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&common_mosquitto_config.config_file)
}

// To confirm the connected c8y tenant is the one that user configured.
#[cfg(feature = "c8y")]
async fn tenant_matches_configured_url(
    tedge_config: &TEdgeConfig,
    profile_name: Option<&ProfileName>,
    configured_mqtt_url: &str,
    configured_http_url: &str,
) -> Result<bool, Fancy<ConnectError>> {
    let spinner = Spinner::start("Checking Cumulocity is connected to intended tenant");
    let c8y_config = tedge_config.mapper_config::<C8yMapperSpecificConfig>(&profile_name)?;
    let res = get_connected_c8y_url(tedge_config, &c8y_config).await;
    match spinner.finish(res) {
        Ok(url) if url == configured_mqtt_url || url == configured_http_url => Ok(true),
        Ok(url) => {
            warning!(
            "The device is connected to {}, but the configured URL is (mqtt={}, http={}).\
            \n    To connect the device to the intended tenant, remove the device certificate from {url}, and then run `tedge reconnect c8y`", 
            url.bold(),
            configured_mqtt_url.bold(),
            configured_http_url.bold(),
        );
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::QoS::AtLeastOnce;

    mod is_bridge_health_up_message {
        use super::*;

        #[test]
        fn health_message_up_is_detected_successfully() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message(health_topic, "up");
            assert!(is_bridge_health_up_message(&message, health_topic, true))
        }

        #[test]
        fn message_on_wrong_topic_is_ignored() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message("a/different/topic", "up");
            assert!(!is_bridge_health_up_message(&message, health_topic, true))
        }

        #[test]
        fn health_message_down_is_ignored() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message(health_topic, "down");
            assert!(!is_bridge_health_up_message(&message, health_topic, true))
        }

        fn test_message(topic: &str, status: &str) -> rumqttc::Publish {
            let payload = serde_json::json!({ "status": status}).to_string();
            rumqttc::Publish::new(topic, AtLeastOnce, payload)
        }
    }

    mod validate_config {
        use super::super::validate_config;
        use super::Cloud;
        use tedge_config::TEdgeConfig;
        use tedge_test_utils::fs::TempTedgeDir;

        #[tokio::test]
        async fn allows_default_config() {
            let cloud = Cloud::C8y(None);
            let config = TEdgeConfig::load_toml_str("");

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_single_named_c8y_profile_without_default_profile() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let config = TEdgeConfig::load_toml_str("c8y.profiles.new.url = \"example.com\"");

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn disallows_matching_device_id_same_urls() {
            yansi::disable();
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let config = TEdgeConfig::load_toml_str(
                "c8y.url = \"example.com\"
            c8y.profiles.new.url = \"example.com\"",
            );

            let err = validate_config(&config, &cloud).unwrap_err();
            pretty_assertions::assert_eq!(err.to_string(), "You have matching URLs and device IDs for different profiles.

c8y.url, c8y.profiles.new.url are set to the same value, but so are c8y.device.id, c8y.profiles.new.device.id.

Each cloud profile requires either a unique URL or unique device ID, so it corresponds to a unique device in the associated cloud.")
        }

        #[tokio::test]
        async fn disallows_matching_device_id_with_separate_mapper_configs() {
            yansi::disable();
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            ttd.dir("mappers")
                .file("c8y.toml")
                .with_raw_content("url = \"example.com\"");
            ttd.dir("mappers/c8y.d")
                .file("new.toml")
                .with_raw_content("url = \"example.com\"");
            let config = TEdgeConfig::load(ttd.path()).await.unwrap();

            let err = validate_config(&config, &cloud).unwrap_err();
            pretty_assertions::assert_eq!(err.to_string(), format!("You have matching URLs and device IDs for different profiles.

c8y.url, c8y.profiles.new.url are set to the same value, but so are c8y.device.id, c8y.profiles.new.device.id.

Each cloud profile requires either a unique URL or unique device ID, so it corresponds to a unique device in the associated cloud."))
        }

        #[tokio::test]
        async fn specifies_relevant_device_id_for_seperate_mapper_config_same_id_error() {
            yansi::disable();
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            ttd.dir("mappers")
                .file("c8y.toml")
                .with_raw_content("url = \"example.com\"\ndevice.id = \"my-device\"");
            ttd.dir("mappers/c8y.d")
                .file("new.toml")
                .with_raw_content("url = \"example.com\"\ndevice.id = \"my-device\"");
            let config = TEdgeConfig::load(ttd.path()).await.unwrap();

            let err = validate_config(&config, &cloud).unwrap_err();
            pretty_assertions::assert_eq!(err.to_string(), format!("You have matching URLs and device IDs for different profiles.

c8y.url, c8y.profiles.new.url are set to the same value, but so are c8y.device.id, c8y.profiles.new.device.id.

Each cloud profile requires either a unique URL or unique device ID, so it corresponds to a unique device in the associated cloud."))
        }

        #[tokio::test]
        async fn allows_different_urls() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let config = TEdgeConfig::load_toml_str(
                "c8y.url = \"example.com\"
            c8y.profiles.new.url = \"different.example.com\"
            c8y.profiles.new.bridge.topic_prefix = \"c8y-new\"
            c8y.profiles.new.proxy.bind.port = 8002",
            );

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_different_device_ids() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let cert = rcgen::generate_simple_self_signed(["test-device".into()]).unwrap();
            let mut cert_path = ttd.utf8_path().to_owned();
            cert_path.push("test.crt");
            let mut key_path = ttd.utf8_path().to_owned();
            key_path.push("test.key");
            std::fs::write(&cert_path, cert.cert.pem()).unwrap();
            std::fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
            let config = TEdgeConfig::load_toml_str_with_root_dir(
                ttd.path(),
                &format!(
                    "c8y.url = \"example.com\"
            c8y.profiles.new.url = \"example.com\"
            c8y.profiles.new.device.id = \"test-device\"
            c8y.profiles.new.device.cert_path = \"{cert_path}\"
            c8y.profiles.new.device.key_path = \"{key_path}\"
            c8y.profiles.new.bridge.topic_prefix = \"c8y-new\"
            c8y.profiles.new.proxy.bind.port = 8002"
                ),
            );

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_combination_of_urls_and_device_ids() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let mut cert_path = ttd.utf8_path().to_owned();
            cert_path.push("test.crt");
            let mut key_path = ttd.utf8_path().to_owned();
            key_path.push("test.key");
            let cert = rcgen::generate_simple_self_signed(["test-device".into()]).unwrap();
            std::fs::write(&cert_path, cert.cert.pem()).unwrap();
            std::fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
            let config = TEdgeConfig::load_toml_str_with_root_dir(
                ttd.path(),
                &format!(
                    "c8y.url = \"example.com\"
            c8y.profiles.diff_id.url = \"example.com\"
            c8y.profiles.diff_id.device.id = \"test-device-second\"
            c8y.profiles.diff_id.device.cert_path = \"{cert_path}\"
            c8y.profiles.diff_id.device.key_path = \"{key_path}\"
            c8y.profiles.diff_id.bridge.topic_prefix = \"c8y-diff-id\"
            c8y.profiles.diff_id.proxy.bind.port = 8002
            "
                ),
            );

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_single_named_az_profile_without_default_profile() {
            let cloud = Cloud::az(Some("new".parse().unwrap()));
            let config = TEdgeConfig::load_toml_str("az.profiles.new.url = \"example.com\"");

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_single_named_aws_profile_without_default_profile() {
            let cloud = Cloud::aws(Some("new".parse().unwrap()));
            let config = TEdgeConfig::load_toml_str("aws.profiles.new.url = \"example.com\"");

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn rejects_conflicting_topic_prefixes() {
            let cloud = Cloud::C8y(None);
            let config = TEdgeConfig::load_toml_str(
                "c8y.url = \"latest.example.com\"
            c8y.profiles.new.url = \"example.com\"
            c8y.profiles.new.proxy.bind.port = 8002",
            );

            let err = validate_config(&config, &cloud).unwrap_err();
            eprintln!("err={err}");
            assert!(err.to_string().contains("c8y.bridge.topic_prefix"));
            assert!(err
                .to_string()
                .contains("c8y.profiles.new.bridge.topic_prefix"));
        }

        #[tokio::test]
        async fn rejects_conflicting_bind_ports() {
            let cloud = Cloud::C8y(None);
            let config = TEdgeConfig::load_toml_str(
                "c8y.url = \"latest.example.com\"
            c8y.profiles.new.url = \"example.com\"
            c8y.profiles.new.bridge.topic_prefix = \"c8y-new\"",
            );

            let err = validate_config(&config, &cloud).unwrap_err();
            eprintln!("err={err}");
            assert!(err.to_string().contains("c8y.proxy.bind.port"));
            assert!(err.to_string().contains("c8y.profiles.new.proxy.bind.port"));
        }

        #[tokio::test]
        async fn ignores_conflicting_configs_for_other_clouds() {
            let cloud = Cloud::Azure(None);
            let config = TEdgeConfig::load_toml_str(
                "c8y.url = \"latest.example.com\"
                c8y.profiles.new.url = \"example.com\"",
            );

            validate_config(&config, &cloud).unwrap();
        }

        #[tokio::test]
        async fn allows_non_conflicting_topic_prefixes() {
            let cloud = Cloud::Azure(None);
            let config =
                TEdgeConfig::load_toml_str("az.profiles.new.bridge.topic_prefix = \"az-new\"");

            validate_config(&config, &cloud).unwrap();
        }
    }
}
