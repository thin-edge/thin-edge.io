use crate::cli::connect::*;
use crate::command::{Command, ExecutionContext};
use crate::config::ConfigError;
use mqtt_client::{Client, Message, Topic, TopicFilter};
use std::time::Duration;
use tedge_config::*;
use tokio::time::timeout;

pub struct ConnectAzureCommand {
    pub config_repository: TEdgeConfigRepository,
}

impl Command for ConnectAzureCommand {
    fn description(&self) -> String {
        "create bridge to connect Azure cloud.".into()
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config_repository.load()?;
        assign_default(&mut config, AzureRootCertPathSetting)?;
        let bridge_config = create_azure_bridge_config(&config)?;
        self.config_repository.store(config)?;

        bridge_config.new_bridge(&context.user_manager)?;
        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        Ok(check_connection()?)
    }
}

pub const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

fn create_azure_bridge_config(config: &TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
    let params = BridgeConfigParams {
        connect_url: config.query(C8yUrlSetting)?,
        mqtt_tls_port: MQTT_TLS_PORT,
        config_file: AZURE_CONFIG_FILENAME.into(),
        bridge_root_cert_path: config.query(C8yRootCertPathSetting)?,
        remote_clientid: config.query(DeviceIdSetting)?,
        bridge_certfile: config.query(DeviceCertPathSetting)?,
        bridge_keyfile: config.query(DeviceKeyPathSetting)?,
    };

    Ok(BridgeConfig::new_for_azure(params))
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

// Here We check the az device twin properties over mqtt to check if connection has been open.
// First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
// device twin property output.
// Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
// The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
// Here if the status is 200 then it's success.

#[tokio::main]
async fn check_connection() -> Result<(), ConnectError> {
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
