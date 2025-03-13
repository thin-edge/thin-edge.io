use crate::bridge::aws::BridgeConfigAwsParams;
use crate::bridge::azure::BridgeConfigAzureParams;
use crate::bridge::c8y::BridgeConfigC8yParams;
use crate::bridge::BridgeConfig;
use crate::bridge::BridgeLocation;
use crate::bridge::CommonMosquittoConfig;
use crate::bridge::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::cli::common::Cloud;
use crate::cli::common::MaybeBorrowedCloud;
use crate::cli::connect::jwt_token::*;
use crate::cli::connect::*;
use crate::cli::log::ConfigLogger;
use crate::cli::log::Fancy;
use crate::cli::log::Spinner;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::system_services::*;
use crate::warning;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::bail;
use c8y_api::http_proxy::read_c8y_credentials;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message_ids::JWT_TOKEN;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use mqtt_channel::Topic;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::TopicIdError;
use tedge_api::service_health_topic;
use tedge_config::auth_method::AuthType;
use tedge_config::TEdgeConfig;
use tedge_config::*;
use tedge_utils::paths::create_directories;
use tedge_utils::paths::ok_if_not_found;
use tedge_utils::paths::DraftFile;
use tracing::warn;
use which::which;
use yansi::Paint as _;

pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 20;
const MQTT_TLS_PORT: u16 = 8883;

pub struct ConnectCommand {
    pub config_location: TEdgeConfigLocation,
    pub config: TEdgeConfig,
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

impl Command for ConnectCommand {
    fn description(&self) -> String {
        if self.is_test_connection {
            format!("test connection to {} cloud.", self.cloud)
        } else {
            format!("connect to {} cloud.", self.cloud)
        }
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.execute_inner().map_err(<_>::into)
    }
}

impl ConnectCommand {
    fn execute_inner(&self) -> Result<(), Fancy<ConnectError>> {
        let config = &self.config;
        let bridge_config = bridge_config(config, &self.cloud)?;
        let updated_mosquitto_config = CommonMosquittoConfig::from_tedge_config(config);
        let credentials_path = credentials_path_for(config, &self.cloud)?;

        validate_config(config, &self.cloud)?;

        if self.is_test_connection {
            ConfigLogger::log(
                format!("Testing {} connection with config", self.cloud),
                &bridge_config,
                &*self.service_manager,
                &self.cloud,
                credentials_path,
            );
            // If the bridge is part of the mapper, the bridge config file won't exist
            // TODO tidy me up once mosquitto is no longer required for bridge
            return if self.check_if_bridge_exists(&bridge_config) {
                match self.check_connection(config) {
                    Ok(DeviceStatus::AlreadyExists) => {
                        let cloud = bridge_config.cloud_name;
                        match self.tenant_matches_configured_url(config)? {
                            // Check failed, warning has been printed already
                            // Don't tell them the connection test succeeded because that's not true
                            Some(false) => {}
                            // Either the check succeeded or it wasn't relevant (e.g. non-Cumulocity connection)
                            Some(true) | None => {
                                println!("Connection check to {cloud} cloud is successful.")
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
            };
        }

        let title = match self.is_reconnect {
            false => format!("Connecting to {} with config", self.cloud),
            true => "Reconnecting with config".into(),
        };
        ConfigLogger::log(
            title,
            &bridge_config,
            &*self.service_manager,
            &self.cloud,
            credentials_path,
        );

        let device_type = &config.device.ty;

        let profile_name = if let Cloud::C8y(profile_name) = &self.cloud {
            profile_name.as_ref().map(|p| p.to_string())
        } else {
            None
        };

        match new_bridge(
            &bridge_config,
            &updated_mosquitto_config,
            self.service_manager.as_ref(),
            &self.config_location,
            device_type,
            self.offline_mode,
            config,
            profile_name.as_deref(),
        ) {
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
            self.start_mapper();
        }

        let mut connection_check_success = true;
        if !self.offline_mode {
            match self
                .check_connection_with_retries(config, bridge_config.connection_check_attempts)
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
            self.start_mapper();
        }

        if let Cloud::C8y(profile) = &self.cloud {
            let c8y_config = config.c8y.try_get(profile.as_deref())?;

            let use_basic_auth = c8y_config
                .auth_method
                .is_basic(&c8y_config.credentials_path);
            if !use_basic_auth && !self.offline_mode && connection_check_success {
                let _ = self.tenant_matches_configured_url(config);
            }
            enable_software_management(&bridge_config, self.service_manager.as_ref());
        }

        Ok(())
    }
}

fn credentials_path_for<'a>(
    config: &'a TEdgeConfig,
    cloud: &Cloud,
) -> Result<Option<&'a Utf8Path>, MultiError> {
    if let Cloud::C8y(profile) = cloud {
        let c8y_config = config.c8y.try_get(profile.as_deref())?;
        Ok(Some(&c8y_config.credentials_path))
    } else {
        Ok(None)
    }
}

impl ConnectCommand {
    fn tenant_matches_configured_url(
        &self,
        config: &TEdgeConfig,
    ) -> Result<Option<bool>, Fancy<ConnectError>> {
        if let Cloud::C8y(profile) = &self.cloud {
            let c8y_config = config.c8y.try_get(profile.as_deref())?;

            let use_basic_auth = c8y_config
                .auth_method
                .is_basic(&c8y_config.credentials_path);
            if !use_basic_auth && !self.offline_mode {
                tenant_matches_configured_url(
                    config,
                    profile.as_ref().map(|g| &***g),
                    &c8y_config
                        .mqtt
                        .or_none()
                        .map(|u| u.host().to_string())
                        .unwrap_or_default(),
                )
                .map(Some)
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn check_connection_with_retries(
        &self,
        config: &TEdgeConfig,
        max_attempts: i32,
    ) -> Result<DeviceStatus, Fancy<ConnectError>> {
        for i in 1..max_attempts {
            let result = self.check_connection(config);
            if let Ok(DeviceStatus::AlreadyExists) = result {
                return result;
            }
            println!(
                "Connection test failed, attempt {} of {}\n",
                i, max_attempts,
            );
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        self.check_connection(config)
    }
    fn check_connection(&self, config: &TEdgeConfig) -> Result<DeviceStatus, Fancy<ConnectError>> {
        let spinner = Spinner::start("Verifying device is connected to cloud");
        let res = match &self.cloud {
            Cloud::Azure(profile) => check_device_status_azure(config, profile.as_deref()),
            Cloud::Aws(profile) => check_device_status_aws(config, profile.as_deref()),
            Cloud::C8y(profile) => check_device_status_c8y(config, profile.as_deref()),
        };
        spinner.finish(res)
    }

    fn check_if_bridge_exists(&self, br_config: &BridgeConfig) -> bool {
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&*br_config.config_file);

        Path::new(&bridge_conf_path).exists()
    }

    fn start_mapper(&self) {
        if which("tedge-mapper").is_err() {
            warning!("tedge-mapper is not installed.");
        } else {
            let spinner = Spinner::start(format!("Enabling {}", self.cloud.mapper_service()));
            let _ = spinner.finish(
                self.service_manager
                    .as_ref()
                    .start_and_enable_service(self.cloud.mapper_service()),
            );
        }
    }
}

fn validate_config(config: &TEdgeConfig, cloud: &MaybeBorrowedCloud<'_>) -> anyhow::Result<()> {
    match cloud {
        MaybeBorrowedCloud::Aws(_) => {
            let profiles = config
                .aws
                .entries()
                .filter(|(_, config)| config.url.or_none().is_some())
                .map(|(s, _)| Some(s?.to_string()))
                .collect::<Vec<_>>();
            disallow_matching_url_device_id(
                config,
                ReadableKey::AwsUrl,
                ReadableKey::AwsDeviceId,
                &profiles,
            )?;
            disallow_matching_configurations(config, ReadableKey::AwsBridgeTopicPrefix, &profiles)?;
        }
        MaybeBorrowedCloud::Azure(_) => {
            let profiles = config
                .az
                .entries()
                .filter(|(_, config)| config.url.or_none().is_some())
                .map(|(s, _)| Some(s?.to_string()))
                .collect::<Vec<_>>();
            disallow_matching_url_device_id(
                config,
                ReadableKey::AzUrl,
                ReadableKey::AzDeviceId,
                &profiles,
            )?;
            disallow_matching_configurations(config, ReadableKey::AzBridgeTopicPrefix, &profiles)?;
        }
        MaybeBorrowedCloud::C8y(_) => {
            let profiles = config
                .c8y
                .entries()
                .filter(|(_, config)| config.http.or_none().is_some())
                .map(|(s, _)| Some(s?.to_string()))
                .collect::<Vec<_>>();
            disallow_matching_url_device_id(
                config,
                ReadableKey::C8yUrl,
                ReadableKey::C8yDeviceId,
                &profiles,
            )?;
            disallow_matching_configurations(config, ReadableKey::C8yBridgeTopicPrefix, &profiles)?;
            disallow_matching_configurations(config, ReadableKey::C8yProxyBindPort, &profiles)?;
        }
    }
    Ok(())
}

fn disallow_matching_url_device_id(
    config: &TEdgeConfig,
    url: fn(Option<String>) -> ReadableKey,
    device_id: fn(Option<String>) -> ReadableKey,
    profiles: &[Option<String>],
) -> anyhow::Result<()> {
    let url_entries = profiles.iter().map(|profile| {
        let key = url(profile.clone());
        let value = config.read_string(&key).ok();
        ((profile, key), value)
    });

    for url_matches in find_all_matching(url_entries) {
        let device_id_entries = profiles.iter().filter_map(|profile| {
            url_matches.iter().find(|(p, _)| *p == profile)?;
            let key = device_id(profile.clone());
            let value = config.read_string(&key).ok();
            Some(((profile, key), value))
        });
        if let Some(matches) = find_matching(device_id_entries) {
            let url_keys: String = matches
                .iter()
                .map(|&(k, _)| format!("{}", url(k.clone()).yellow().bold()))
                .collect::<Vec<_>>()
                .join(", ");
            let device_id_keys: String = matches
                .iter()
                .map(|(_, key)| format!("{}", key.yellow().bold()))
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

fn disallow_matching_configurations(
    config: &TEdgeConfig,
    configuration: fn(Option<String>) -> ReadableKey,
    profiles: &[Option<String>],
) -> anyhow::Result<()> {
    let keys = profiles
        .iter()
        .cloned()
        .map(configuration)
        .collect::<Vec<_>>();
    let entries = keys.into_iter().filter_map(|key| {
        let value = config.read_string(&key).ok()?;
        Some((key, value))
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

pub fn bridge_config(
    config: &TEdgeConfig,
    cloud: &MaybeBorrowedCloud<'_>,
) -> Result<BridgeConfig, ConfigError> {
    let bridge_location = match config.mqtt.bridge.built_in {
        true => BridgeLocation::BuiltIn,
        false => BridgeLocation::Mosquitto,
    };
    let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());
    match cloud {
        MaybeBorrowedCloud::Azure(profile) => {
            let az_config = config.az.try_get(profile.as_deref())?;

            let params = BridgeConfigAzureParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    az_config.url.or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: az_config.root_cert_path.clone(),
                remote_clientid: az_config.device.id()?.clone(),
                bridge_certfile: az_config.device.cert_path.clone(),
                bridge_keyfile: az_config.device.key_path.clone(),
                bridge_location,
                topic_prefix: az_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: az_config.bridge.keepalive_interval.duration(),
            };

            Ok(BridgeConfig::from(params))
        }
        MaybeBorrowedCloud::Aws(profile) => {
            let aws_config = config.aws.try_get(profile.as_deref())?;

            let params = BridgeConfigAwsParams {
                mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from(
                    aws_config.url.or_config_not_set()?.as_str(),
                )
                .map_err(TEdgeConfigError::from)?,
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: aws_config.root_cert_path.clone(),
                remote_clientid: aws_config.device.id()?.clone(),
                bridge_certfile: aws_config.device.cert_path.clone(),
                bridge_keyfile: aws_config.device.key_path.clone(),
                bridge_location,
                topic_prefix: aws_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: aws_config.bridge.keepalive_interval.duration(),
            };

            Ok(BridgeConfig::from(params))
        }
        MaybeBorrowedCloud::C8y(profile) => {
            let c8y_config = config.c8y.try_get(profile.as_deref())?;

            let (remote_username, remote_password) =
                match c8y_config.auth_method.to_type(&c8y_config.credentials_path) {
                    AuthType::Certificate => (None, None),
                    AuthType::Basic => {
                        let (username, password) =
                            read_c8y_credentials(&c8y_config.credentials_path)?;
                        (Some(username), Some(password))
                    }
                };

            let params = BridgeConfigC8yParams {
                mqtt_host: c8y_config.mqtt.or_config_not_set()?.clone(),
                config_file: cloud.bridge_config_filename(),
                bridge_root_cert_path: c8y_config.root_cert_path.clone(),
                remote_clientid: c8y_config.device.id()?.clone(),
                remote_username,
                remote_password,
                bridge_certfile: c8y_config.device.cert_path.clone(),
                bridge_keyfile: c8y_config.device.key_path.clone(),
                smartrest_templates: c8y_config.smartrest.templates.clone(),
                smartrest_one_templates: c8y_config.smartrest1.templates.clone(),
                include_local_clean_session: c8y_config.bridge.include.local_cleansession.clone(),
                bridge_location,
                topic_prefix: c8y_config.bridge.topic_prefix.clone(),
                profile_name: profile.clone().map(Cow::into_owned),
                mqtt_schema,
                keepalive_interval: c8y_config.bridge.keepalive_interval.duration(),
                use_cryptoki: config.device.cryptoki.enable,
            };

            Ok(BridgeConfig::from(params))
        }
    }
}

fn bridge_health_topic(
    prefix: &TopicPrefix,
    tedge_config: &TEdgeConfig,
) -> Result<Topic, TopicIdError> {
    let bridge_name = format!("tedge-mapper-bridge-{prefix}");
    let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
    let device_topic_id = tedge_config.mqtt.device_topic_id.parse::<EntityTopicId>()?;
    Ok(service_health_topic(
        &mqtt_schema,
        &device_topic_id,
        &bridge_name,
    ))
}

fn is_bridge_health_up_message(message: &rumqttc::Publish, health_topic: &str) -> bool {
    message.topic == health_topic
        && std::str::from_utf8(&message.payload).is_ok_and(|msg| msg.contains("\"up\""))
}

// Check the connection by using the jwt token retrieval over the mqtt.
// If successful in getting the jwt token '71,xxxxx', the connection is established.
fn check_device_status_c8y(
    tedge_config: &TEdgeConfig,
    c8y_profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;

    // TODO: Use SmartREST1 to check connection
    if c8y_config
        .auth_method
        .is_basic(&c8y_config.credentials_path)
    {
        return Ok(DeviceStatus::AlreadyExists);
    }

    let prefix = &c8y_config.bridge.topic_prefix;
    let built_in_bridge_health = bridge_health_topic(prefix, tedge_config).unwrap().name;
    let c8y_topic_builtin_jwt_token_downstream = format!("{prefix}/s/dat");
    let c8y_topic_builtin_jwt_token_upstream = format!("{prefix}/s/uat");
    const CLIENT_ID: &str = "check_connection_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    connection
        .eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;

    let built_in_bridge = tedge_config.mqtt.bridge.built_in;
    if built_in_bridge {
        client.subscribe(&built_in_bridge_health, AtLeastOnce)?;
    }
    client.subscribe(&c8y_topic_builtin_jwt_token_downstream, AtLeastOnce)?;

    let mut err = None;
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    &c8y_topic_builtin_jwt_token_upstream,
                    rumqttc::QoS::AtMostOnce,
                    false,
                    "",
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic == c8y_topic_builtin_jwt_token_downstream {
                    // We got a response
                    let response = std::str::from_utf8(&response.payload).unwrap();
                    let message_id = get_smartrest_template_id(response);
                    if message_id.parse() == Ok(JWT_TOKEN) {
                        break;
                    }
                } else if is_bridge_health_up_message(&response, &built_in_bridge_health) {
                    client.publish(
                        &c8y_topic_builtin_jwt_token_upstream,
                        rumqttc::QoS::AtMostOnce,
                        false,
                        "",
                    )?;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive response from Cumulocity")
                } else {
                    anyhow!("Local MQTT publish has timed out")
                });
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto during connection check"
                ));
                break;
            }
            Err(e) => {
                err = Some(
                    anyhow::Error::from(e)
                        .context("Failed to connect to mosquitto for connection check"),
                );
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    match err {
        None => Ok(DeviceStatus::AlreadyExists),
        // The request has been sent but without a response
        Some(_) if acknowledged => Ok(DeviceStatus::Unknown),
        // The request has not even been sent
        Some(err) => Err(err
            .context("Failed to verify device is connected to Cumulocity")
            .into()),
    }
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.
fn check_device_status_azure(
    tedge_config: &TEdgeConfig,
    profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let az_config = tedge_config.az.try_get(profile)?;
    let topic_prefix = &az_config.bridge.topic_prefix;
    let built_in_bridge_health = bridge_health_topic(topic_prefix, tedge_config)
        .unwrap()
        .name;
    let azure_topic_device_twin_downstream = format!(r##"{topic_prefix}/twin/res/#"##);
    let azure_topic_device_twin_upstream = format!(r#"{topic_prefix}/twin/GET/?$rid=1"#);
    const CLIENT_ID: &str = "check_connection_az";
    const REGISTRATION_PAYLOAD: &[u8] = b"";
    const REGISTRATION_OK: &str = "200";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;

    if tedge_config.mqtt.bridge.built_in {
        client.subscribe(&built_in_bridge_health, AtLeastOnce)?;
    }
    client.subscribe(azure_topic_device_twin_downstream, AtLeastOnce)?;

    let mut err = None;
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    &azure_topic_device_twin_upstream,
                    AtLeastOnce,
                    false,
                    REGISTRATION_PAYLOAD,
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic.contains(REGISTRATION_OK) {
                    // We got a response
                    break;
                } else if response.topic == built_in_bridge_health {
                    client.publish(
                        &azure_topic_device_twin_upstream,
                        AtLeastOnce,
                        false,
                        REGISTRATION_PAYLOAD,
                    )?;
                } else {
                    break;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive response from Azure")
                } else {
                    anyhow!("Local MQTT publish has timed out")
                });
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto during connection check"
                ));
                break;
            }
            Err(e) => {
                err = Some(
                    anyhow::Error::from(e)
                        .context("Failed to connect to mosquitto for connection check"),
                );
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    match err {
        None => Ok(DeviceStatus::AlreadyExists),
        // The request has been sent but without a response
        Some(_) if acknowledged => Ok(DeviceStatus::Unknown),
        // The request has not even been sent
        Some(err) => Err(err
            .context("Failed to verify device is connected to Azure")
            .into()),
    }
}

fn check_device_status_aws(
    tedge_config: &TEdgeConfig,
    profile: Option<&ProfileName>,
) -> Result<DeviceStatus, ConnectError> {
    let aws_config = tedge_config.aws.try_get(profile)?;
    let topic_prefix = &aws_config.bridge.topic_prefix;
    let aws_topic_pub_check_connection = format!("{topic_prefix}/test-connection");
    let aws_topic_sub_check_connection = format!("{topic_prefix}/connection-success");
    let built_in_bridge_health = bridge_health_topic(topic_prefix, tedge_config)
        .unwrap()
        .name;
    const CLIENT_ID: &str = "check_connection_aws";
    const REGISTRATION_PAYLOAD: &[u8] = b"";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;

    if tedge_config.mqtt.bridge.built_in {
        client.subscribe(&built_in_bridge_health, AtLeastOnce)?;
    }
    client.subscribe(&aws_topic_sub_check_connection, AtLeastOnce)?;

    let mut err = None;
    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    &aws_topic_pub_check_connection,
                    AtLeastOnce,
                    false,
                    REGISTRATION_PAYLOAD,
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                if response.topic == aws_topic_sub_check_connection {
                    // We got a response
                    break;
                } else if is_bridge_health_up_message(&response, &built_in_bridge_health) {
                    // Built in bridge is now up, republish the message in case it was never received by the bridge
                    client.publish(
                        &aws_topic_pub_check_connection,
                        AtLeastOnce,
                        false,
                        REGISTRATION_PAYLOAD,
                    )?;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                err = Some(if acknowledged {
                    anyhow!("Didn't receive response from AWS")
                } else {
                    anyhow!("Local MQTT publish has timed out")
                });
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                err = Some(anyhow!(
                    "Client was disconnected from mosquitto during connection check"
                ));
                break;
            }
            Err(e) => {
                err = Some(
                    anyhow::Error::from(e)
                        .context("Failed to connect to mosquitto for connection check"),
                );
                break;
            }
            _ => {}
        }
    }

    // Cleanly disconnect client
    client.disconnect()?;
    for event in connection.iter() {
        match event {
            Ok(Event::Outgoing(Outgoing::Disconnect)) | Err(_) => break,
            _ => {}
        }
    }

    match err {
        None => Ok(DeviceStatus::AlreadyExists),
        // The request has been sent but without a response
        Some(_) if acknowledged => Ok(DeviceStatus::Unknown),
        // The request has not even been sent
        Some(err) => Err(err
            .context("Failed to verify device is connected to AWS")
            .into()),
    }
}

// TODO: too many args
#[allow(clippy::too_many_arguments)]
fn new_bridge(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
    device_type: &str,
    offline_mode: bool,
    tedge_config: &TEdgeConfig,
    // TODO(marcel): remove this argument
    profile_name: Option<&str>,
) -> Result<(), Fancy<ConnectError>> {
    let service_manager_result = service_manager.check_operational();

    if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
        &service_manager_result
    {
        warning!("'{name}' service manager is not available on the system.",);
    }

    fail_if_already_connected(config_location, bridge_config)?;

    let use_basic_auth =
        bridge_config.remote_username.is_some() && bridge_config.remote_password.is_some();

    let mqtt_auth_config = tedge_config.mqtt_auth_config_cloud_broker(profile_name)?;

    if bridge_config.cloud_name.eq("c8y") {
        if offline_mode {
            println!("Offline mode. Skipping device creation in Cumulocity cloud.")
        } else {
            let spinner = Spinner::start("Creating device in Cumulocity cloud");
            let res = c8y_direct_connection::create_device_with_direct_connection(
                use_basic_auth,
                bridge_config,
                device_type,
                mqtt_auth_config,
            );
            spinner.finish(res)?;
        }
    }

    if let Err(err) =
        write_generic_mosquitto_config_to_file(config_location, common_mosquitto_config)
    {
        // We want to preserve previous errors and therefore discard result of this function.
        let _ = clean_up(config_location, bridge_config);
        return Err(err.into());
    }

    if bridge_config.bridge_location == BridgeLocation::Mosquitto {
        if let Err(err) = write_bridge_config_to_file(config_location, bridge_config) {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = clean_up(config_location, bridge_config);
            return Err(err.into());
        }
    } else {
        use_built_in_bridge(config_location, bridge_config)?;
    }

    if let Err(err) = service_manager_result {
        println!("'tedge connect' configured the necessary tedge components, but you will have to start the required services on your own.");
        println!("Start/restart mosquitto and other thin edge components.");
        println!("thin-edge.io works seamlessly with 'systemd'.\n");
        return Err(err.into());
    }

    restart_mosquitto(bridge_config, service_manager, config_location)?;

    let spinner = Spinner::start("Waiting for mosquitto to be listening for connections");
    spinner.finish(wait_for_mosquitto_listening(&tedge_config.mqtt))?;

    if let Err(err) = service_manager.enable_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    Ok(())
}

pub fn chown_certificate_and_key(bridge_config: &BridgeConfig) {
    let (user, group) = match bridge_config.bridge_location {
        BridgeLocation::BuiltIn => ("tedge", "tedge"),
        BridgeLocation::Mosquitto => (crate::BROKER_USER, crate::BROKER_GROUP),
    };
    // Ignore errors - This was the behavior with the now deprecated user manager.
    // - When `tedge cert create` is not run as root, a certificate is created but owned by the user running the command.
    // - A better approach could be to remove this `chown` and run the command as mosquitto.
    let path = &bridge_config.bridge_certfile;
    if let Err(err) = tedge_utils::file::change_user_and_group(path.as_ref(), user, group) {
        warn!("Failed to change ownership of {path} to {user}:{group}: {err}");
    }

    // if using cryptoki, no private key to chown
    if bridge_config.use_cryptoki {
        return;
    }

    let path = &bridge_config.bridge_keyfile;
    if let Err(err) = tedge_utils::file::change_user_and_group(path.as_ref(), user, group) {
        warn!("Failed to change ownership of {path} to {user}:{group}: {err}");
    }
}

fn restart_mosquitto(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
) -> Result<(), Fancy<ConnectError>> {
    let spinner = Spinner::start("Restarting mosquitto");
    spinner
        .finish(restart_mosquitto_inner(bridge_config, service_manager))
        .inspect_err(|_| {
            // We want to preserve existing errors and therefore discard result of this function.
            let _ = clean_up(config_location, bridge_config);
        })
}
fn restart_mosquitto_inner(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) -> Result<(), ConnectError> {
    service_manager.stop_service(SystemService::Mosquitto)?;
    chown_certificate_and_key(bridge_config);
    service_manager.restart_service(SystemService::Mosquitto)?;

    Ok(())
}

fn wait_for_mosquitto_listening(mqtt: &TEdgeConfigReaderMqtt) -> Result<(), anyhow::Error> {
    let addr = format!("{}:{}", mqtt.client.host, mqtt.client.port);
    if let Some(addr) = addr.to_socket_addrs().ok().and_then(|mut o| o.next()) {
        let timeout = Duration::from_secs(MOSQUITTO_RESTART_TIMEOUT_SECONDS);
        let min_loop_time = Duration::from_millis(10);
        let connect = || TcpStream::connect_timeout(&addr, Duration::from_secs(1));
        match retry_until_success(connect, timeout, min_loop_time) {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!(
                "Timed out after {timeout:?} waiting for mosquitto to be listening"
            )),
        }
    } else {
        bail!("Couldn't resolve configured mosquitto address");
    }
}

fn enable_software_management(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) {
    if bridge_config.use_agent {
        if which("tedge-agent").is_ok() {
            let spinner = Spinner::start("Enabling tedge-agent");
            let _ = spinner
                .finish(service_manager.start_and_enable_service(SystemService::TEdgeSMAgent));
        } else {
            println!("Info: Software management is not installed. So, skipping enabling related components.\n");
        }
    }
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
pub fn clean_up(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), Fancy<ConnectError>> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    Spinner::start(format!("Cleaning up {path} due to failure"))
        .finish(std::fs::remove_file(path).or_else(ok_if_not_found))?;
    Ok(())
}

pub fn use_built_in_bridge(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    std::fs::write(
        path,
        "# This file is left empty as the built-in bridge is enabled",
    )
    .or_else(ok_if_not_found)?;
    Ok(())
}

fn fail_if_already_connected(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    if path.exists() {
        return Err(ConnectError::ConfigurationExists {
            cloud: bridge_config.cloud_name.to_string(),
        });
    }
    Ok(())
}

fn write_generic_mosquitto_config_to_file(
    config_location: &TEdgeConfigLocation,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Result<(), ConnectError> {
    let dir_path = config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let common_config_path =
        get_common_mosquitto_config_file_path(config_location, common_mosquitto_config);
    let mut common_draft = DraftFile::new(common_config_path)?.with_mode(0o644);
    common_mosquitto_config.serialize(&mut common_draft)?;
    common_draft.persist()?;

    Ok(())
}

fn write_bridge_config_to_file(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let dir_path = config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH);

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    create_directories(dir_path)?;

    let config_path = get_bridge_config_file_path(config_location, bridge_config);
    let mut config_draft = DraftFile::new(config_path)?.with_mode(0o644);
    bridge_config.serialize(&mut config_draft)?;
    config_draft.persist()?;

    Ok(())
}

fn get_bridge_config_file_path(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&*bridge_config.config_file)
}

fn get_common_mosquitto_config_file_path(
    config_location: &TEdgeConfigLocation,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Utf8PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&common_mosquitto_config.config_file)
}

// To confirm the connected c8y tenant is the one that user configured.
fn tenant_matches_configured_url(
    tedge_config: &TEdgeConfig,
    c8y_prefix: Option<&str>,
    configured_url: &str,
) -> Result<bool, Fancy<ConnectError>> {
    let spinner = Spinner::start("Checking Cumulocity is connected to intended tenant");
    let res = get_connected_c8y_url(tedge_config, c8y_prefix);
    match spinner.finish(res) {
        Ok(url) if url == configured_url => Ok(true),
        Ok(url) => {
            warning!(
            "The device is connected to {}, but the configured URL is {}.\
            \n    To connect the device to the intended tenant, remove the device certificate from {url}, and then run `tedge reconnect c8y`", 
            url.bold(),
            configured_url.bold(),
        );
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
struct Timeout;

fn retry_until_success<T, E>(
    mut action: impl FnMut() -> Result<T, E>,
    timeout: Duration,
    min_loop_time: Duration,
) -> Result<T, Timeout> {
    let start = Instant::now();
    loop {
        let start_loop = Instant::now();
        if let Ok(res) = action() {
            return Ok(res);
        }

        if start.elapsed() > timeout {
            return Err(Timeout);
        }

        if start_loop.elapsed() < min_loop_time {
            std::thread::sleep(min_loop_time - start_loop.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod is_bridge_health_up_message {
        use super::*;

        #[test]
        fn health_message_up_is_detected_successfully() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message(health_topic, "up");
            assert!(is_bridge_health_up_message(&message, health_topic))
        }

        #[test]
        fn message_on_wrong_topic_is_ignored() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message("a/different/topic", "up");
            assert!(!is_bridge_health_up_message(&message, health_topic))
        }

        #[test]
        fn health_message_down_is_ignored() {
            let health_topic = "te/device/main/service/tedge-mapper-bridge-c8y/status/health";
            let message = test_message(health_topic, "down");
            assert!(!is_bridge_health_up_message(&message, health_topic))
        }

        fn test_message(topic: &str, status: &str) -> rumqttc::Publish {
            let payload = serde_json::json!({ "status": status}).to_string();
            rumqttc::Publish::new(topic, AtLeastOnce, payload)
        }
    }

    mod retry_until_success {
        use super::*;

        #[test]
        fn returns_okay_upon_immediate_success() {
            assert_eq!(
                retry_until_success(|| Ok::<_, ()>(()), secs(5), secs(0)),
                Ok(())
            )
        }

        #[test]
        fn returns_okay_upon_retry_success() {
            let mut results = [Err(()), Ok(())].into_iter();
            assert_eq!(
                retry_until_success(move || results.next().unwrap(), secs(5), secs(0)),
                Ok(())
            )
        }

        #[test]
        fn returns_timeout_on_failure() {
            assert_eq!(
                retry_until_success(|| Err::<(), _>(()), millis(50), secs(0)),
                Err(Timeout)
            )
        }

        #[test]
        fn avoids_hard_looping() {
            let mut results = [Err(()), Ok(())].into_iter();
            let min_loop_time = millis(50);
            let start = Instant::now();
            retry_until_success(|| results.next().unwrap(), secs(5), min_loop_time).unwrap();
            assert!(dbg!(start.elapsed()) > min_loop_time);
        }

        fn secs(n: u64) -> Duration {
            Duration::from_secs(n)
        }

        fn millis(n: u64) -> Duration {
            Duration::from_millis(n)
        }
    }

    mod validate_config {
        use super::super::validate_config;
        use super::Cloud;
        use tedge_config::TEdgeConfigLocation;
        use tedge_test_utils::fs::TempTedgeDir;

        #[test]
        fn allows_default_config() {
            let cloud = Cloud::C8y(None);
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_single_named_c8y_profile_without_default_profile() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn disallows_matching_device_id_same_urls() {
            yansi::disable();
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            let err = validate_config(&config, &cloud).unwrap_err();
            assert_eq!(err.to_string(), "You have matching URLs and device IDs for different profiles.

c8y.url, c8y.profiles.new.url are set to the same value, but so are c8y.device.id, c8y.profiles.new.device.id.

Each cloud profile requires either a unique URL or unique device ID, so it corresponds to a unique device in the associated cloud.")
        }

        #[test]
        fn allows_different_urls() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.url".parse().unwrap(),
                    "different.example.com",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.bridge.topic_prefix".parse().unwrap(),
                    "c8y-new",
                )
                .unwrap();
                dto.try_update_str(&"c8y.profiles.new.proxy.bind.port".parse().unwrap(), "8002")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_different_device_ids() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let cert = rcgen::generate_simple_self_signed(["test-device".into()]).unwrap();
            let mut cert_path = ttd.path().to_owned();
            cert_path.push("test.crt");
            let mut key_path = ttd.path().to_owned();
            key_path.push("test.key");
            std::fs::write(&cert_path, cert.serialize_pem().unwrap()).unwrap();
            std::fs::write(&key_path, cert.serialize_private_key_pem()).unwrap();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.device.id".parse().unwrap(),
                    "test-device",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.device.cert_path".parse().unwrap(),
                    &cert_path.display().to_string(),
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.device.key_path".parse().unwrap(),
                    &key_path.display().to_string(),
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.bridge.topic_prefix".parse().unwrap(),
                    "c8y-new",
                )
                .unwrap();
                dto.try_update_str(&"c8y.profiles.new.proxy.bind.port".parse().unwrap(), "8002")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_combination_of_urls_and_device_ids() {
            let cloud = Cloud::c8y(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let cert = rcgen::generate_simple_self_signed(["test-device".into()]).unwrap();
            let mut cert_path = ttd.path().to_owned();
            cert_path.push("test.crt");
            let mut key_path = ttd.path().to_owned();
            key_path.push("test.key");
            std::fs::write(&cert_path, cert.serialize_pem().unwrap()).unwrap();
            std::fs::write(&key_path, cert.serialize_private_key_pem()).unwrap();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.diff_id.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_id.device.id".parse().unwrap(),
                    "test-device-second",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_id.device.cert_path".parse().unwrap(),
                    &cert_path.display().to_string(),
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_id.device.key_path".parse().unwrap(),
                    &key_path.display().to_string(),
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_id.bridge.topic_prefix".parse().unwrap(),
                    "c8y-diff-id",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_id.proxy.bind.port".parse().unwrap(),
                    "8002",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_url.url".parse().unwrap(),
                    "different.example.com",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_url.bridge.topic_prefix".parse().unwrap(),
                    "c8y-diff-url",
                )
                .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.diff_url.proxy.bind.port".parse().unwrap(),
                    "8003",
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_single_named_az_profile_without_default_profile() {
            let cloud = Cloud::az(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"az.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_single_named_aws_profile_without_default_profile() {
            let cloud = Cloud::aws(Some("new".parse().unwrap()));
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"aws.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn rejects_conflicting_topic_prefixes() {
            let cloud = Cloud::C8y(None);
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "latest.example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.proxy.bind.port".parse().unwrap(), "8002")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            let err = validate_config(&config, &cloud).unwrap_err();
            eprintln!("err={err}");
            assert!(err.to_string().contains("c8y.bridge.topic_prefix"));
            assert!(err
                .to_string()
                .contains("c8y.profiles.new.bridge.topic_prefix"));
        }

        #[test]
        fn rejects_conflicting_bind_ports() {
            let cloud = Cloud::C8y(None);
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "latest.example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                dto.try_update_str(
                    &"c8y.profiles.new.bridge.topic_prefix".parse().unwrap(),
                    "c8y-new",
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            let err = validate_config(&config, &cloud).unwrap_err();
            eprintln!("err={err}");
            assert!(err.to_string().contains("c8y.proxy.bind.port"));
            assert!(err.to_string().contains("c8y.profiles.new.proxy.bind.port"));
        }

        #[test]
        fn ignores_conflicting_configs_for_other_clouds() {
            let cloud = Cloud::Azure(None);
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(&"c8y.url".parse().unwrap(), "latest.example.com")
                    .unwrap();
                dto.try_update_str(&"c8y.profiles.new.url".parse().unwrap(), "example.com")
                    .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }

        #[test]
        fn allows_non_conflicting_topic_prefixes() {
            let cloud = Cloud::Azure(None);
            let ttd = TempTedgeDir::new();
            let loc = TEdgeConfigLocation::from_custom_root(ttd.path());
            loc.update_toml(&|dto, _| {
                dto.try_update_str(
                    &"az.profiles.new.bridge.topic_prefix".parse().unwrap(),
                    "az-new",
                )
                .unwrap();
                Ok(())
            })
            .unwrap();
            let config = loc.load().unwrap();

            validate_config(&config, &cloud).unwrap();
        }
    }
}
