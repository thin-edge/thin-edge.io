use crate::cli::connect::*;
use crate::command::{Command, ExecutionContext};
use crate::services::{
    self, mosquitto::MosquittoService, tedge_mapper::TedgeMapperService, SystemdService,
};
use crate::utils::paths;
use crate::utils::users::UserManager;
use crate::ConfigError;
use mqtt_client::{Client, Message, Topic, TopicFilter};
use std::path::Path;
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
    pub config_repository: TEdgeConfigRepository,
    pub cloud: Cloud,
    pub common_mosquitto_config: CommonMosquittoConfig,
}

pub enum Cloud {
    Azure,
    C8y,
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
        format!("create bridge to connect {} cloud.", self.cloud.as_str())
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config_repository.load()?;
        // XXX: Do we really need to persist the defaults?
        match self.cloud {
            Cloud::Azure => assign_default(&mut config, AzureRootCertPathSetting)?,
            Cloud::C8y => assign_default(&mut config, C8yRootCertPathSetting)?,
        }
        let bridge_config = self.bridge_config(&config)?;
        self.config_repository.store(config)?;

        new_bridge(
            &bridge_config,
            &self.common_mosquitto_config,
            &context.user_manager,
        )?;

        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        Ok(self.check_connection()?)
    }
}

impl ConnectCommand {
    fn bridge_config(&self, config: &TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        match self.cloud {
            Cloud::Azure => {
                let params = BridgeConfigAzureParams {
                    connect_url: config.query(C8yUrlSetting)?,
                    mqtt_tls_port: MQTT_TLS_PORT,
                    config_file: AZURE_CONFIG_FILENAME.into(),
                    bridge_root_cert_path: config.query(C8yRootCertPathSetting)?,
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

    fn check_connection(&self) -> Result<(), ConnectError> {
        match self.cloud {
            Cloud::Azure => check_connection_azure(),
            Cloud::C8y => check_connection_c8y(),
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
async fn check_connection_c8y() -> Result<(), ConnectError> {
    const C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM: &str = "c8y/s/us";
    const C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM: &str = "c8y/s/e";
    const CLIENT_ID: &str = "check_connection_c8y";

    let c8y_msg_pub_topic = Topic::new(C8Y_TOPIC_BUILTIN_MESSAGE_UPSTREAM)?;
    let c8y_error_sub_topic = Topic::new(C8Y_TOPIC_ERROR_MESSAGE_DOWNSTREAM)?;

    let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
    let mut error_response = mqtt.subscribe(c8y_error_sub_topic.filter()).await?;

    let (sender, mut receiver) = tokio::sync::oneshot::channel();

    let _task_handle = tokio::spawn(async move {
        while let Some(message) = error_response.next().await {
            if std::str::from_utf8(&message.payload)
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

        // 100: Device creation
        mqtt.publish(Message::new(&c8y_msg_pub_topic, "100"))
            .await?;

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

    println!("Warning: Bridge has been configured, but Cumulocity connection check failed.\n",);
    Ok(())
}

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.

#[tokio::main]
async fn check_connection_azure() -> Result<(), ConnectError> {
    const AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
    const AZURE_TOPIC_DEVICE_TWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
    const CLIENT_ID: &str = "check_connection_az";

    let device_twin_pub_topic = Topic::new(AZURE_TOPIC_DEVICE_TWIN_UPSTREAM)?;
    let device_twin_sub_filter = TopicFilter::new(AZURE_TOPIC_DEVICE_TWIN_DOWNSTREAM)?;

    let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
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

    mqtt.publish(Message::new(&device_twin_pub_topic, "".to_string()))
        .await?;

    let fut = timeout(RESPONSE_TIMEOUT, &mut receiver);
    match fut.await {
        Ok(Ok(true)) => {
            println!("Received expected response message, connection check is successful.");
            Ok(())
        }
        _err => {
            println!("Warning: No response, bridge has been configured, but Azure connection check failed.\n",);
            Ok(())
        }
    }
}

fn new_bridge(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
    user_manager: &UserManager,
) -> Result<(), ConnectError> {
    println!("Checking if systemd is available.\n");
    let () = services::systemd_available()?;

    println!("Checking if configuration for requested bridge already exists.\n");
    let () = bridge_config_exists(bridge_config)?;

    println!("Validating the bridge certificates.\n");
    let () = bridge_config.validate()?;

    println!("Saving configuration for requested bridge.\n");
    if let Err(err) = write_bridge_config_to_file(bridge_config, common_mosquitto_config) {
        // We want to preserve previous errors and therefore discard result of this function.
        let _ = clean_up(bridge_config);
        return Err(err);
    }

    println!("Restarting mosquitto service.\n");
    if let Err(err) = MosquittoService.restart(user_manager) {
        clean_up(bridge_config)?;
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
    if let Err(err) = MosquittoService.enable(user_manager) {
        clean_up(bridge_config)?;
        return Err(err.into());
    }

    println!("Successfully created bridge connection!\n");

    if bridge_config.use_mapper {
        println!("Checking if tedge-mapper is installed.\n");

        if which("tedge_mapper").is_err() {
            println!("Warning: tedge_mapper is not installed. We recommend to install it.\n");
        } else {
            start_and_enable_tedge_mapper(user_manager);
        }
    }

    Ok(())
}

// To preserve error chain and not discard other errors we need to ignore error here
// (don't use '?' with the call to this function to preserve original error).
fn clean_up(bridge_config: &BridgeConfig) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(bridge_config)?;
    let _ = std::fs::remove_file(&path).or_else(ok_if_not_found)?;
    Ok(())
}

fn bridge_config_exists(bridge_config: &BridgeConfig) -> Result<(), ConnectError> {
    let path = get_bridge_config_file_path(bridge_config)?;
    if Path::new(&path).exists() {
        return Err(ConnectError::ConfigurationExists {
            cloud: bridge_config.cloud_name.to_string(),
        });
    }
    Ok(())
}

fn write_bridge_config_to_file(
    bridge_config: &BridgeConfig,
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Result<(), ConnectError> {
    let dir_path = paths::build_path_for_sudo_or_user(&[TEDGE_BRIDGE_CONF_DIR_PATH])?;

    // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
    let _ = paths::create_directories(&dir_path)?;

    let mut common_temp_file = NamedTempFile::new()?;
    common_mosquitto_config.serialize(&mut common_temp_file)?;
    let common_config_path = get_common_mosquitto_config_file_path(common_mosquitto_config)?;
    let () = paths::persist_tempfile(common_temp_file, &common_config_path)?;

    let mut temp_file = NamedTempFile::new()?;
    bridge_config.serialize(&mut temp_file)?;
    let config_path = get_bridge_config_file_path(bridge_config)?;
    let () = paths::persist_tempfile(temp_file, &config_path)?;

    Ok(())
}

fn get_bridge_config_file_path(bridge_config: &BridgeConfig) -> Result<String, ConnectError> {
    Ok(paths::build_path_for_sudo_or_user(&[
        TEDGE_BRIDGE_CONF_DIR_PATH,
        &bridge_config.config_file,
    ])?)
}

fn start_and_enable_tedge_mapper(user_manager: &UserManager) {
    let mut failed = false;

    println!("Starting tedge-mapper service.\n");
    if let Err(err) = TedgeMapperService.restart(user_manager) {
        println!("Failed to stop tedge-mapper service: {:?}", err);
        failed = true;
    }

    println!("Persisting tedge-mapper on reboot.\n");
    if let Err(err) = TedgeMapperService.enable(user_manager) {
        println!("Failed to enable tedge-mapper service: {:?}", err);
        failed = true;
    }

    if !failed {
        println!("tedge-mapper service successfully started and enabled!\n");
    }
}

fn get_common_mosquitto_config_file_path(
    common_mosquitto_config: &CommonMosquittoConfig,
) -> Result<String, ConnectError> {
    Ok(paths::build_path_for_sudo_or_user(&[
        TEDGE_BRIDGE_CONF_DIR_PATH,
        &common_mosquitto_config.config_file,
    ])?)
}

fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}
