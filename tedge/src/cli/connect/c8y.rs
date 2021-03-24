use super::*;
use crate::config::ConfigError;
use crate::utils::config;
use mqtt_client::{Client, Message, Topic};
use std::time::Duration;
use tokio::time::timeout;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
pub const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";

pub struct C8y {}

impl C8y {
    pub fn c8y_bridge_config(mut config: TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        Self::assign_bridge_root_cert_path(&mut config)?;
        config.write_to_default_config()?;
        Self::new_config(&config)
    }

    pub fn assign_bridge_root_cert_path(config: &mut TEdgeConfig) -> Result<(), ConfigError> {
        let bridge_root_cert_path = config::get_config_value_or_default(
            &config,
            C8Y_ROOT_CERT_PATH,
            DEFAULT_ROOT_CERT_PATH,
        )?;
        let _ = config.set_config_value(C8Y_ROOT_CERT_PATH, DEFAULT_ROOT_CERT_PATH.into())?;
        Ok(())
    }

    pub fn new_config(config: &TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        let address = format!(
            "{}:{}",
            config::parse_user_provided_address(config::get_config_value(&config, C8Y_URL)?)?,
            MQTT_TLS_PORT
        );

        Ok(BridgeConfig {
            common_bridge_config: CommonBridgeConfig::default(),
            cloud_name: "c8y".into(),
            config_file: C8Y_CONFIG_FILENAME.to_string(),
            connection: "edge_to_c8y".into(),
            address,
            remote_username: None,
            bridge_root_cert_path: config::get_config_value(&config, C8Y_ROOT_CERT_PATH)?,
            remote_clientid: config::get_config_value(&config, DEVICE_ID)?,
            local_clientid: "Cumulocity".into(),
            bridge_certfile: config::get_config_value(&config, DEVICE_CERT_PATH)?,
            bridge_keyfile: config::get_config_value(&config, DEVICE_KEY_PATH)?,
            topics: vec![
                // Registration
                r#"s/dcr in 2 c8y/ """#.into(),
                r#"s/ucr out 2 c8y/ """#.into(),
                // Templates
                r#"s/dt in 2 c8y/ """#.into(),
                r#"s/ut/# out 2 c8y/ """#.into(),
                // Static templates
                r#"s/us out 2 c8y/ """#.into(),
                r#"t/us out 2 c8y/ """#.into(),
                r#"q/us out 2 c8y/ """#.into(),
                r#"c/us out 2 c8y/ """#.into(),
                r#"s/ds in 2 c8y/ """#.into(),
                r#"s/os in 2 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 2 c8y/ """#.into(),
                r#"t/uc/# out 2 c8y/ """#.into(),
                r#"q/uc/# out 2 c8y/ """#.into(),
                r#"c/uc/# out 2 c8y/ """#.into(),
                r#"s/dc/# in 2 c8y/ """#.into(),
                r#"s/oc/# in 2 c8y/ """#.into(),
                // c8y JSON
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
                r#"error in 2 c8y/ """#.into(),
            ],
        })
    }

    // Check the connection by using the response of the SmartREST template 100.
    // If getting the response '41,100,Device already existing', the connection is established.
    //
    // If the device is already registered, it can finish in the first try.
    // If the device is new, the device is going to be registered here and
    // the check can finish in the second try as there is no error response in the first try.

    #[tokio::main]
    async fn check_connection_async(&self) -> Result<(), ConnectError> {
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
                    println!(
                        "Received expected response message, connection check is successful\n",
                    );
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
}

impl CheckConnection for C8y {
    fn check_connection(&self) -> Result<(), ConnectError> {
        Ok(self.check_connection_async()?)
    }
}
