use crate::{cli::connect::*, command::Command, system_services::*, utils::paths, ConfigError};
use mqtt_client::{Client, Message, MqttClient, Topic, TopicFilter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tedge_config::*;
use tempfile::NamedTempFile;
use tokio::time::timeout;
use which::which;

const WAIT_FOR_CHECK_SECONDS: u64 = 10;
const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
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
            format!("create bridge to connect {} cloud.", self.cloud.as_str())
        }
    }

    fn execute(&self) -> anyhow::Result<()> {
        let mut config = self.config_repository.load()?;

        if self.is_test_connection {
            return match self.check_connection(&config) {
                Ok(()) => Ok(()),
                Err(err) => Err(err.into()),
            };
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
            .with_port(config.query(MqttPortSetting)?.into());
        self.config_repository.store(&config)?;

        new_bridge(
            &bridge_config,
            &self.cloud,
            &updated_mosquitto_config,
            self.service_manager.as_ref(),
            &self.config_location,
        )?;

        if self.check_connection(&config).is_err() {
            println!(
                "Warning: Bridge has been configured, but {} connection check failed.\n",
                self.cloud.as_str()
            );
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

    fn check_connection(&self, config: &TEdgeConfig) -> Result<(), ConnectError> {
        let port = config.query(MqttPortSetting)?.into();
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        match self.cloud {
            Cloud::Azure => check_connection_azure(port),
            Cloud::C8y => check_connection_c8y(port),
        }
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

#[tokio::main]
async fn check_connection_c8y(port: u16) -> Result<(), ConnectError> {
    const C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM: &str = "c8y/s/us";
    const C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM: &str = "c8y/s/e";
    const CLIENT_ID: &str = "check_connection_c8y";

    let c8y_msg_pub_topic = Topic::new(C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM)?;
    let c8y_error_sub_topic = Topic::new(C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM)?;

    let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default().with_port(port)).await?;
    let mut error_response = mqtt.subscribe(c8y_error_sub_topic.filter()).await?;

    let (sender, mut receiver) = tokio::sync::oneshot::channel();

    let _task_handle = tokio::spawn(async move {
        while let Some(message) = error_response.next().await {
            if message
                .payload_str()
                .unwrap_or("")
                .contains("41,100,Device already existing")
            {
                let _ = sender.send(true);
                break;
            }
        }
    });

    for i in 0..2 {
        print!("Try {} / 2: Sending a message to Cumulocity. ", i + 1,);

        if timeout(
            RESPONSE_TIMEOUT,
            // 100: Device creation
            mqtt.publish(Message::new(&c8y_msg_pub_topic, "100")),
        )
        .await
        .is_err()
        {
            println!("\nLocal MQTT publish has timed out. Make sure mosquitto is running.");
            return Err(ConnectError::TimeoutElapsedError);
        }

        let fut = timeout(RESPONSE_TIMEOUT, &mut receiver);
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received expected response message, connection check is successful.\n",);
                return Ok(());
            }
            _err => {
                if i == 0 {
                    println!("... No response. If the device is new, it's normal to get no response in the first try.");
                } else {
                    println!("... No response. ");
                }
            }
        }
    }
    Err(ConnectError::NoPacketsReceived {
        cloud: "Cumulocity".into(),
    })
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.

#[tokio::main]
async fn check_connection_azure(port: u16) -> Result<(), ConnectError> {
    const AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
    const AZURE_TOPIC_DEVICE_TWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
    const CLIENT_ID: &str = "check_connection_az";

    let device_twin_pub_topic = Topic::new(AZURE_TOPIC_DEVICE_TWIN_UPSTREAM)?;
    let device_twin_sub_filter = TopicFilter::new(AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM)?;

    let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default().with_port(port)).await?;
    let mut device_twin_response = mqtt.subscribe(device_twin_sub_filter).await?;

    let (sender, mut receiver) = tokio::sync::oneshot::channel();

    let _task_handle = tokio::spawn(async move {
        if let Some(message) = device_twin_response.next().await {
            //status should be 200 for successful connection
            if message.topic.name.contains("200") {
                let _ = sender.send(true);
            } else {
                let _ = sender.send(false);
            }
        }
    });

    if timeout(
        RESPONSE_TIMEOUT,
        mqtt.publish(Message::new(&device_twin_pub_topic, "".to_string())),
    )
    .await
    .is_err()
    {
        println!("\nLocal MQTT publish has timed out. Make sure mosquitto is running.");
        return Err(ConnectError::TimeoutElapsedError);
    }

    let fut = timeout(RESPONSE_TIMEOUT, &mut receiver);
    match fut.await {
        Ok(Ok(true)) => {
            println!("Received expected response message, connection check is successful.");
            Ok(())
        }
        _err => Err(ConnectError::NoPacketsReceived {
            cloud: "Azure".into(),
        }),
    }
}

fn new_bridge(
    bridge_config: &BridgeConfig,
    cloud: &Cloud,
    common_mosquitto_config: &CommonMosquittoConfig,
    service_manager: &dyn SystemServiceManager,
    config_location: &TEdgeConfigLocation,
) -> Result<(), ConnectError> {
    println!("Checking if {} is available.\n", service_manager.name());
    let () = service_manager.check_operational()?;

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

    println!("Restarting mosquitto service.\n");
    if let Err(err) = service_manager.restart_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    println!(
        "Awaiting mosquitto to start. This may take up to {} seconds.\n",
        MOSQUITTO_RESTART_TIMEOUT_SECONDS
    );
    std::thread::sleep(std::time::Duration::from_secs(
        MOSQUITTO_RESTART_TIMEOUT_SECONDS,
    ));

    println!("Persisting mosquitto on reboot.\n");
    if let Err(err) = service_manager.enable_service(SystemService::Mosquitto) {
        clean_up(config_location, bridge_config)?;
        return Err(err.into());
    }

    println!("Successfully created bridge connection!\n");

    if bridge_config.use_mapper {
        println!("Checking if tedge-mapper is installed.\n");

        if which("tedge_mapper").is_err() {
            println!("Warning: tedge_mapper is not installed. We recommend to install it.\n");
        } else {
            service_manager
                .start_and_enable_service(cloud.dependent_mapper_service(), std::io::stdout());
        }
    }

    Ok(())
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
fn clean_up(
    config_location: &TEdgeConfigLocation,
    bridge_config: &BridgeConfig,
) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(config_location, bridge_config);
    let _ = std::fs::remove_file(&path).or_else(paths::ok_if_not_found)?;
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
    let _ = paths::create_directories(&dir_path)?;

    let mut common_temp_file = NamedTempFile::new()?;
    common_mosquitto_config.serialize(&mut common_temp_file)?;
    let common_config_path =
        get_common_mosquitto_config_file_path(config_location, common_mosquitto_config);
    let () = paths::persist_tempfile(common_temp_file, &common_config_path)?;

    let mut temp_file = NamedTempFile::new()?;
    bridge_config.serialize(&mut temp_file)?;
    let config_path = get_bridge_config_file_path(config_location, bridge_config);
    let () = paths::persist_tempfile(temp_file, &config_path)?;

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
