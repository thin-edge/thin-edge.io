use super::*;
use crate::config::ConfigError;
use mqtt_client::{Client, Message, Topic};
use std::time::Duration;
use tokio::time::timeout;

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";

pub struct C8y {}

impl C8y {
    pub fn c8y_bridge_config(config: TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        Ok(BridgeConfig {
            cloud_name: "c8y".into(),
            config_file: C8Y_CONFIG_FILENAME.to_string(),
            connection: "edge_to_c8y".into(),
            address: get_config_value(&config, C8Y_URL)?,
            remote_username: "".into(),
            bridge_cafile: get_config_value(&config, C8Y_ROOT_CERT_PATH)?,
            remote_clientid: get_config_value(&config, DEVICE_ID)?,
            local_clientid: "Cumulocity".into(),
            bridge_certfile: get_config_value(&config, DEVICE_CERT_PATH)?,
            bridge_keyfile: get_config_value(&config, DEVICE_KEY_PATH)?,
            try_private: false,
            start_type: "automatic".into(),
            cleansession: true,
            bridge_insecure: false,
            notifications: false,
            bridge_attempt_unsubscribe: false,
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
        const CLIENT_ID: &str = "check_connection";

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
                        " ... Received message.\nThe device is already registered in Cumulocity.\n",
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
