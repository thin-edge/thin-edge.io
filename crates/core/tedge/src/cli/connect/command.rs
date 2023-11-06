use crate::cli::common::Cloud;
use crate::cli::connect::jwt_token::*;
use crate::cli::connect::*;
use crate::command::Command;
use crate::ConfigError;
use camino::Utf8PathBuf;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::QoS::AtLeastOnce;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::system_services::*;
use tedge_config::TEdgeConfig;
use tedge_config::*;
use tedge_utils::paths::create_directories;
use tedge_utils::paths::ok_if_not_found;
use tedge_utils::paths::DraftFile;
use which::which;

const WAIT_FOR_CHECK_SECONDS: u64 = 2;
const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const AWS_CONFIG_FILENAME: &str = "aws-bridge.conf";
pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(60);
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const MQTT_TLS_PORT: u16 = 8883;
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";

pub struct ConnectCommand {
    pub config_location: TEdgeConfigLocation,
    pub config_repository: TEdgeConfigRepository,
    pub cloud: Cloud,
    pub common_mosquitto_config: CommonMosquittoConfig,
    pub is_test_connection: bool,
    pub service_manager: Arc<dyn SystemServiceManager>,
}

pub enum DeviceStatus {
    AlreadyExists,
    Unknown,
}

impl Command for ConnectCommand {
    fn description(&self) -> String {
        if self.is_test_connection {
            format!("test connection to {} cloud.", self.cloud.as_str())
        } else {
            format!("connect {} cloud.", self.cloud.as_str())
        }
    }

    fn execute(&self) -> anyhow::Result<()> {
        let config = self.config_repository.load()?;
        if self.is_test_connection {
            let br_config = self.bridge_config(&config)?;
            if self.check_if_bridge_exists(&br_config) {
                return match self.check_connection(&config) {
                    Ok(DeviceStatus::AlreadyExists) => {
                        let cloud = br_config.cloud_name;
                        println!("Connection check to {} cloud is successful.\n", cloud);
                        Ok(())
                    }
                    Ok(DeviceStatus::Unknown) => Err(ConnectError::UnknownDeviceStatus.into()),
                    Err(err) => Err(err.into()),
                };
            } else {
                return Err((ConnectError::DeviceNotConnected {
                    cloud: self.cloud.as_str().into(),
                })
                .into());
            }
        }

        let bridge_config = self.bridge_config(&config)?;
        let updated_mosquitto_config = self
            .common_mosquitto_config
            .clone()
            .with_internal_opts(
                config.mqtt.bind.port.into(),
                config.mqtt.bind.address.to_string(),
            )
            .with_external_opts(
                config.mqtt.external.bind.port.or_none().cloned(),
                config
                    .mqtt
                    .external
                    .bind
                    .address
                    .or_none()
                    .cloned()
                    .map(|a| a.to_string()),
                config.mqtt.external.bind.interface.or_none().cloned(),
                config.mqtt.external.ca_path.or_none().cloned(),
                config.mqtt.external.cert_file.or_none().cloned(),
                config.mqtt.external.key_file.or_none().cloned(),
            );

        let device_type = &config.device.ty;

        match new_bridge(
            &bridge_config,
            &updated_mosquitto_config,
            self.service_manager.as_ref(),
            &self.config_location,
            device_type,
        ) {
            Ok(()) => println!("Successfully created bridge connection!\n"),
            Err(ConnectError::SystemServiceError(
                SystemServiceError::ServiceManagerUnavailable { .. },
            )) => return Ok(()),
            Err(err) => return Err(err.into()),
        }

        match self.check_connection(&config) {
            Ok(DeviceStatus::AlreadyExists) => {
                println!("Connection check is successful.\n");
            }
            _ => {
                println!(
                    "Warning: Bridge has been configured, but {} connection check failed.\n",
                    self.cloud.as_str()
                );
            }
        }

        if bridge_config.use_mapper {
            println!("Checking if tedge-mapper is installed.\n");

            if which("tedge-mapper").is_err() {
                println!("Warning: tedge-mapper is not installed.\n");
            } else {
                self.service_manager
                    .as_ref()
                    .start_and_enable_service(self.cloud.mapper_service(), std::io::stdout());
            }
        }

        if let Cloud::C8y = self.cloud {
            check_connected_c8y_tenant_as_configured(
                &config,
                &config
                    .c8y
                    .mqtt
                    .or_none()
                    .cloned()
                    .map(|u| u.to_string())
                    .unwrap_or_default(),
            );
            enable_software_management(&bridge_config, self.service_manager.as_ref());
        }

        Ok(())
    }
}

impl ConnectCommand {
    fn bridge_config(&self, config: &TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        match self.cloud {
            Cloud::Azure => {
                let params = BridgeConfigAzureParams {
                    connect_url: config.az.url.or_config_not_set()?.clone(),
                    mqtt_tls_port: MQTT_TLS_PORT,
                    config_file: AZURE_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.az.root_cert_path.clone(),
                    remote_clientid: config.device.id.try_read(config)?.clone(),
                    bridge_certfile: config.device.cert_path.clone(),
                    bridge_keyfile: config.device.key_path.clone(),
                };

                Ok(BridgeConfig::from(params))
            }
            Cloud::Aws => {
                let params = BridgeConfigAwsParams {
                    connect_url: config.aws.url.or_config_not_set()?.clone(),
                    mqtt_tls_port: MQTT_TLS_PORT,
                    config_file: AWS_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.aws.root_cert_path.clone(),
                    remote_clientid: config.device.id.try_read(config)?.clone(),
                    bridge_certfile: config.device.cert_path.clone(),
                    bridge_keyfile: config.device.key_path.clone(),
                };

                Ok(BridgeConfig::from(params))
            }
            Cloud::C8y => {
                let params = BridgeConfigC8yParams {
                    mqtt_host: config.c8y.mqtt.or_config_not_set()?.clone(),
                    config_file: C8Y_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.c8y.root_cert_path.clone(),
                    remote_clientid: config.device.id.try_read(config)?.clone(),
                    bridge_certfile: config.device.cert_path.clone(),
                    bridge_keyfile: config.device.key_path.clone(),
                    smartrest_templates: config.c8y.smartrest.templates.clone(),
                    include_local_clean_session: config
                        .c8y
                        .bridge
                        .include
                        .local_cleansession
                        .clone(),
                };

                Ok(BridgeConfig::from(params))
            }
        }
    }

    fn check_connection(&self, config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        match self.cloud {
            Cloud::Azure => check_device_status_azure(config),
            Cloud::Aws => check_device_status_aws(config),
            Cloud::C8y => check_device_status_c8y(config),
        }
    }

    fn check_if_bridge_exists(&self, br_config: &BridgeConfig) -> bool {
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(br_config.config_file.clone());

        Path::new(&bridge_conf_path).exists()
    }
}

// Check the connection by using the jwt token retrieval over the mqtt.
// If successful in getting the jwt token '71,xxxxx', the connection is established.
fn check_device_status_c8y(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    const C8Y_TOPIC_BUILTIN_JWT_TOKEN_DOWNSTREAM: &str = "c8y/s/dat";
    const C8Y_TOPIC_BUILTIN_JWT_TOKEN_UPSTREAM: &str = "c8y/s/uat";
    const CLIENT_ID: &str = "check_connection_c8y";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .with_clean_session(true)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    connection
        .eventloop
        .network_options
        .set_connection_timeout(CONNECTION_TIMEOUT.as_secs());
    let mut acknowledged = false;

    client.subscribe(C8Y_TOPIC_BUILTIN_JWT_TOKEN_DOWNSTREAM, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    C8Y_TOPIC_BUILTIN_JWT_TOKEN_UPSTREAM,
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
                // We got a response
                let token = String::from_utf8(response.payload.to_vec()).unwrap();
                if token.contains("71") {
                    return Ok(DeviceStatus::AlreadyExists);
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.
fn check_device_status_azure(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    const AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
    const AZURE_TOPIC_DEVICE_TWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
    const CLIENT_ID: &str = "check_connection_az";
    const REGISTRATION_PAYLOAD: &[u8] = b"";
    const REGISTRATION_OK: &str = "200";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;

    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;

    client.subscribe(AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    AZURE_TOPIC_DEVICE_TWIN_UPSTREAM,
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
                // We got a response
                if response.topic.contains(REGISTRATION_OK) {
                    println!("Received expected response message, connection check is successful.");
                    return Ok(DeviceStatus::AlreadyExists);
                } else {
                    break;
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

fn check_device_status_aws(tedge_config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
    const AWS_TOPIC_PUB_CHECK_CONNECTION: &str = r#"aws/test-connection"#;
    const AWS_TOPIC_SUB_CHECK_CONNECTION: &str = r#"aws/connection-success"#;
    const CLIENT_ID: &str = "check_connection_aws";
    const REGISTRATION_PAYLOAD: &[u8] = b"";

    let mut mqtt_options = tedge_config
        .mqtt_config()?
        .with_session_name(CLIENT_ID)
        .rumqttc_options()?;
    mqtt_options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(mqtt_options, 10);
    let mut acknowledged = false;

    client.subscribe(AWS_TOPIC_SUB_CHECK_CONNECTION, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    AWS_TOPIC_PUB_CHECK_CONNECTION,
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
                // We got a response
                println!(
                    "Received expected response on topic {}, connection check is successful.",
                    response.topic
                );
                return Ok(DeviceStatus::AlreadyExists);
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                eprintln!("ERROR: Local MQTT publish has timed out.");
                break;
            }
            Ok(Event::Incoming(Incoming::Disconnect)) => {
                eprintln!("ERROR: Disconnected");
                break;
            }
            Err(err) => {
                eprintln!("ERROR: {:?}", err);
                break;
            }
            _ => {}
        }
    }

    if acknowledged {
        // The request has been sent but without a response
        Ok(DeviceStatus::Unknown)
    } else {
        // The request has not even been sent
        Err(ConnectError::TimeoutElapsedError)
    }
}

fn new_bridge(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
    device_type: &str,
) -> Result<(), ConnectError> {
    println!("Checking if {} is available.\n", service_manager.name());
    let service_manager_result = service_manager.check_operational();

    if let Err(SystemServiceError::ServiceManagerUnavailable { cmd: _, name }) =
        &service_manager_result
    {
        println!(
            "Warning: '{}' service manager is not available on the system.\n",
            name
        );
    }

    println!("Checking if configuration for requested bridge already exists.\n");
    bridge_config_exists(config_location, bridge_config)?;

    println!("Validating the bridge certificates.\n");
    bridge_config.validate()?;

    if bridge_config.cloud_name.eq("c8y") {
        println!("Creating the device in Cumulocity cloud.\n");
        c8y_direct_connection::create_device_with_direct_connection(bridge_config, device_type)?;
    }

    println!("Saving configuration for requested bridge.\n");
    if let Err(err) =
        write_bridge_config_to_file(config_location, bridge_config, common_mosquitto_config)
    {
        // We want to preserve previous errors and therefore discard result of this function.
        let _ = clean_up(config_location, bridge_config);
        return Err(err);
    }

    if let Err(err) = service_manager_result {
        println!("'tedge connect' configured the necessary tedge components, but you will have to start the required services on your own.");
        println!("Start/restart mosquitto and other thin edge components.");
        println!("thin-edge.io works seamlessly with 'systemd'.\n");
        return Err(err.into());
    }

    restart_mosquitto(bridge_config, service_manager, config_location)?;

    println!(
        "Awaiting mosquitto to start. This may take up to {} seconds.\n",
        MOSQUITTO_RESTART_TIMEOUT_SECONDS
    );
    std::thread::sleep(std::time::Duration::from_secs(
        MOSQUITTO_RESTART_TIMEOUT_SECONDS,
    ));

    println!("Enabling mosquitto service on reboots.\n");
    if let Err(err) = service_manager.enable_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    Ok(())
}

fn restart_mosquitto(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
) -> Result<(), ConnectError> {
    println!("Restarting mosquitto service.\n");
    if let Err(err) = service_manager.restart_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    Ok(())
}

fn enable_software_management(
    bridge_config: &BridgeConfig,
    service_manager: &dyn SystemServiceManager,
) {
    println!("Enabling software management.\n");
    if bridge_config.use_agent {
        println!("Checking if tedge-agent is installed.\n");
        if which("tedge-agent").is_ok() {
            service_manager
                .start_and_enable_service(SystemService::TEdgeSMAgent, std::io::stdout());
        } else {
            println!("Info: Software management is not installed. So, skipping enabling related components.\n");
        }
    }
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
fn clean_up(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    std::fs::remove_file(path).or_else(ok_if_not_found)?;
    Ok(())
}

fn bridge_config_exists(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    if Path::new(&path).exists() {
        return Err(ConnectError::ConfigurationExists {
            cloud: bridge_config.cloud_name.to_string(),
        });
    }
    Ok(())
}

fn write_bridge_config_to_file(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
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
        .join(&bridge_config.config_file)
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
fn check_connected_c8y_tenant_as_configured(tedge_config: &TEdgeConfig, configured_url: &str) {
    match get_connected_c8y_url(tedge_config) {
        Ok(url) if url == configured_url => {}
        Ok(url) => println!(
            "Warning: Connecting to {}, but the configured URL is {}.\n\
            The device certificate has to be removed from the former tenant.\n",
            url, configured_url
        ),
        Err(_) => println!("Failed to get the connected tenant URL from Cumulocity.\n"),
    }
}
