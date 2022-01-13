use crate::{
    cli::connect::jwt_token::*, cli::connect::*, command::Command, system_services::*, ConfigError,
};
use rumqttc::QoS::AtLeastOnce;
use rumqttc::{Event, Incoming, MqttOptions, Outgoing, Packet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tedge_config::*;
use tedge_utils::paths::{create_directories, ok_if_not_found, DraftFile};
use which::which;

pub(crate) const DEFAULT_HOST: &str = "localhost";
const WAIT_FOR_CHECK_SECONDS: u64 = 10;
const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
pub(crate) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
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
    MightBeNew,
    AlreadyExists,
    Unknown,
}

#[derive(Debug)]
pub enum Cloud {
    Azure,
    C8y,
}

impl Cloud {
    fn dependent_mapper_service(&self) -> SystemService {
        match self {
            Cloud::Azure => SystemService::TEdgeMapperAz,
            Cloud::C8y => SystemService::TEdgeMapperC8y,
        }
    }
}

impl Cloud {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Azure => "Azure",
            Self::C8y => "Cumulocity",
        }
    }
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
        let mut config = self.config_repository.load()?;

        if self.is_test_connection {
            let br_config = self.bridge_config(&config)?;
            if self.check_if_bridge_exists(br_config) {
                return match self.check_connection(&config) {
                    Ok(_) => Ok(()),
                    Err(err) => Err(err.into()),
                };
            } else {
                return Err((ConnectError::DeviceNotConnected {
                    cloud: self.cloud.as_str().into(),
                })
                .into());
            }
        }

        // XXX: Do we really need to persist the defaults?
        match self.cloud {
            Cloud::Azure => assign_default(&mut config, AzureRootCertPathSetting)?,
            Cloud::C8y => assign_default(&mut config, C8yRootCertPathSetting)?,
        }
        let bridge_config = self.bridge_config(&config)?;
        let updated_mosquitto_config = self
            .common_mosquitto_config
            .clone()
            .with_internal_opts(config.query(MqttPortSetting)?.into())
            .with_external_opts(
                config.query(MqttExternalPortSetting).ok().map(|x| x.into()),
                config.query(MqttExternalBindAddressSetting).ok(),
                config.query(MqttExternalBindInterfaceSetting).ok(),
                config
                    .query(MqttExternalCAPathSetting)
                    .ok()
                    .map(|x| x.to_string()),
                config
                    .query(MqttExternalCertfileSetting)
                    .ok()
                    .map(|x| x.to_string()),
                config
                    .query(MqttExternalKeyfileSetting)
                    .ok()
                    .map(|x| x.to_string()),
            );
        self.config_repository.store(&config)?;

        new_bridge(
            &bridge_config,
            &updated_mosquitto_config,
            self.service_manager.as_ref(),
            &self.config_location,
        )?;

        match self.check_connection(&config) {
            Ok(DeviceStatus::AlreadyExists) => {}
            Ok(DeviceStatus::MightBeNew) | Ok(DeviceStatus::Unknown) => {
                if let Cloud::C8y = self.cloud {
                    println!("Restarting mosquitto to resubscribe to bridged inbound cloud topics after device creation.\n");
                    restart_mosquitto(
                        &bridge_config,
                        self.service_manager.as_ref(),
                        &self.config_location,
                    )?;

                    println!(
                        "Awaiting mosquitto to start. This may take up to {} seconds.\n",
                        MOSQUITTO_RESTART_TIMEOUT_SECONDS
                    );
                    std::thread::sleep(std::time::Duration::from_secs(
                        MOSQUITTO_RESTART_TIMEOUT_SECONDS,
                    ));
                }
            }
            Err(_) => {
                println!(
                    "Warning: Bridge has been configured, but {} connection check failed.\n",
                    self.cloud.as_str()
                );
            }
        }

        if bridge_config.use_mapper {
            println!("Checking if tedge-mapper is installed.\n");

            if which("tedge_mapper").is_err() {
                println!("Warning: tedge_mapper is not installed.\n");
            } else {
                self.service_manager.as_ref().start_and_enable_service(
                    self.cloud.dependent_mapper_service(),
                    std::io::stdout(),
                );
            }
        }

        if let Cloud::C8y = self.cloud {
            check_connected_c8y_tenant_as_configured(
                &config.query_string(C8yUrlSetting)?,
                config.query(MqttPortSetting)?.into(),
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
                    connect_url: config.query(AzureUrlSetting)?,
                    mqtt_tls_port: MQTT_TLS_PORT,
                    config_file: AZURE_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.query(AzureRootCertPathSetting)?,
                    remote_clientid: config.query(DeviceIdSetting)?,
                    bridge_certfile: config.query(DeviceCertPathSetting)?,
                    bridge_keyfile: config.query(DeviceKeyPathSetting)?,
                };

                Ok(BridgeConfig::from(params))
            }
            Cloud::C8y => {
                let params = BridgeConfigC8yParams {
                    connect_url: config.query(C8yUrlSetting)?,
                    mqtt_tls_port: MQTT_TLS_PORT,
                    config_file: C8Y_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.query(C8yRootCertPathSetting)?,
                    remote_clientid: config.query(DeviceIdSetting)?,
                    bridge_certfile: config.query(DeviceCertPathSetting)?,
                    bridge_keyfile: config.query(DeviceKeyPathSetting)?,
                };

                Ok(BridgeConfig::from(params))
            }
        }
    }

    fn check_connection(&self, config: &TEdgeConfig) -> Result<DeviceStatus, ConnectError> {
        let port = config.query(MqttPortSetting)?.into();
        let device_id = config.query(DeviceIdSetting)?;
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        match self.cloud {
            Cloud::Azure => check_device_status_azure(port),
            Cloud::C8y => check_device_status_c8y(port, device_id.as_str()),
        }
    }

    fn check_if_bridge_exists(&self, br_config: BridgeConfig) -> bool {
        let bridge_conf_path = self
            .config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(br_config.config_file);

        Path::new(&bridge_conf_path).exists()
    }
}

// XXX: Improve naming
fn assign_default<T: ConfigSetting + Copy>(
    config: &mut TEdgeConfig,
    setting: T,
) -> Result<(), ConfigError>
where
    TEdgeConfig: ConfigSettingAccessor<T>,
{
    let value = config.query(setting)?;
    let () = config.update(setting, value)?;
    Ok(())
}

// Check the connection by using the response of the SmartREST template 100.
// If getting the response '41,100,Device already existing', the connection is established.
//
// If the device is already registered, it can finish in the first try.
// If the device is new, the device is going to be registered here and
// the check can finish in the second try as there is no error response in the first try.
fn check_device_status_c8y(port: u16, device_id: &str) -> Result<DeviceStatus, ConnectError> {
    for i in 0..2 {
        println!("Try {} / 2: Sending a message to Cumulocity. ", i + 1,);

        match create_device(port, device_id) {
            Ok(DeviceStatus::MightBeNew) => return Ok(DeviceStatus::MightBeNew),
            Ok(DeviceStatus::AlreadyExists) => {
                println!("Received expected response message, connection check is successful.\n",);
                if i == 0 {
                    // If the DeviceAlreadyExists response comes on the first attempt itself, the device definitely was created earlier
                    return Ok(DeviceStatus::AlreadyExists);
                } else {
                    // If the DeviceAlreadyExists response comes on the second attempt only,
                    // it may have been created on the first attempt or earlier.
                    // The absence of the response on the first attempt can't be considered as definite proof of new device creation
                    // due to the possibility of a DeviceAlreadyExists response from that first attempt getting lost in transit.
                    // So, we could only say that the device might be new, but not entirely sure.
                    return Ok(DeviceStatus::MightBeNew);
                }
            }
            Ok(DeviceStatus::Unknown) => {
                if i == 0 {
                    println!("... No response. If the device is new, it's normal to get no response in the first try.");
                } else {
                    println!("... No response. ");
                    return Err(ConnectError::ConnectionCheckError);
                }
            }
            Err(err) => {
                return Err(err);
            }
        }
    }
    Ok(DeviceStatus::MightBeNew)
}

fn create_device(port: u16, device_id: &str) -> Result<DeviceStatus, ConnectError> {
    const C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM: &str = "c8y/s/us";
    const C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM: &str = "c8y/s/e";
    const CLIENT_ID: &str = "check_connection_c8y";
    const REGISTRATION_ERROR: &[u8] = b"41,100,Device already existing";
    const DEVICE_TYPE: &str = "thin-edge.io";

    let registration_payload = format!("100,{},{}", device_id, DEVICE_TYPE);

    let mut options = MqttOptions::new(CLIENT_ID, DEFAULT_HOST, port);
    options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(options, 10);
    let mut acknowledged = false;

    client.subscribe(C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM, AtLeastOnce)?;

    for event in connection.iter() {
        match event {
            Ok(Event::Incoming(Packet::SubAck(_))) => {
                // We are ready to get the response, hence send the request
                client.publish(
                    C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM,
                    AtLeastOnce,
                    false,
                    registration_payload.as_bytes(),
                )?;
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                // The request has been sent
                acknowledged = true;
            }
            Ok(Event::Incoming(Packet::Publish(response))) => {
                // We got a response
                if response.payload == REGISTRATION_ERROR {
                    return Ok(DeviceStatus::AlreadyExists);
                }
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                // No messages have been received for a while
                println!("Local MQTT publish has timed out.");
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
        println!("\nMake sure mosquitto is running.");
        Err(ConnectError::TimeoutElapsedError)
    }
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.
fn check_device_status_azure(port: u16) -> Result<DeviceStatus, ConnectError> {
    const AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
    const AZURE_TOPIC_DEVICE_TWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
    const CLIENT_ID: &str = "check_connection_az";
    const REGISTRATION_PAYLOAD: &[u8] = b"";
    const REGISTRATION_OK: &str = "200";

    let mut options = MqttOptions::new(CLIENT_ID, DEFAULT_HOST, port);
    options.set_keep_alive(RESPONSE_TIMEOUT);

    let (mut client, mut connection) = rumqttc::Client::new(options, 10);
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
                println!("Local MQTT publish has timed out.");
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
        println!("Make sure mosquitto is running.");
        Err(ConnectError::TimeoutElapsedError)
    }
}

fn new_bridge(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
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
    let () = bridge_config_exists(config_location, bridge_config)?;

    println!("Validating the bridge certificates.\n");
    let () = bridge_config.validate()?;

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

    println!("Successfully created bridge connection!\n");

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
        if which("tedge_agent").is_ok() {
            service_manager
                .start_and_enable_service(SystemService::TEdgeSMAgent, std::io::stdout());
            service_manager
                .start_and_enable_service(SystemService::TEdgeSMMapperC8Y, std::io::stdout());
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
    let _ = std::fs::remove_file(&path).or_else(ok_if_not_found)?;
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
    let _ = create_directories(&dir_path)?;

    let common_config_path =
        get_common_mosquitto_config_file_path(config_location, common_mosquitto_config);
    let mut common_draft = DraftFile::new(&common_config_path)?;
    common_mosquitto_config.serialize(&mut common_draft)?;
    let () = common_draft.persist()?;

    let config_path = get_bridge_config_file_path(config_location, bridge_config);
    let mut config_draft = DraftFile::new(config_path)?;
    bridge_config.serialize(&mut config_draft)?;
    let () = config_draft.persist()?;

    Ok(())
}

fn get_bridge_config_file_path(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&bridge_config.config_file)
}

fn get_common_mosquitto_config_file_path(
    config_location: &TEdgeConfigLocation,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> PathBuf {
    config_location
        .tedge_config_root_path
        .join(TEDGE_BRIDGE_CONF_DIR_PATH)
        .join(&common_mosquitto_config.config_file)
}

// To confirm the connected c8y tenant is the one that user configured.
fn check_connected_c8y_tenant_as_configured(configured_url: &str, port: u16) {
    match get_connected_c8y_url(port) {
        Ok(url) if url == configured_url => {}
        Ok(url) => println!(
            "Warning: Connecting to {}, but the configured URL is {}.\n\
            The device certificate has to be removed from the former tenant.\n",
            url, configured_url
        ),
        Err(_) => println!("Failed to get the connected tenant URL from Cumulocity.\n"),
    }
}
