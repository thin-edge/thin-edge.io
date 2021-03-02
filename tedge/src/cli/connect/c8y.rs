use super::*;
use mqtt_client::{Client, Message, Topic};
use std::time::Duration;
use tokio::time::timeout;

const MOSQUITTO_RESTART_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct C8y {}

impl C8y {
    pub fn c8y_bridge_config() -> Result<BridgeConfig, ConnectError> {
        let config = TEdgeConfig::from_default_config()?;
        Ok(BridgeConfig {
            cloud_type: TEdgeConnectOpt::C8y,
            connection: "edge_to_c8y".into(),
            address: get_config_value(&config, C8Y_URL)?,
            remote_username: "".into(),
            bridge_cafile: get_config_value(&config, C8Y_ROOT_CERT_PATH)?,
            remote_clientid: get_config_value(&config, DEVICE_ID)?,
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

    pub async fn check_connection() -> Result<(), ConnectError> {
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

/*
#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";

    #[test]
    fn config_c8y_validate_ok() {
        let ca_file = NamedTempFile::new().unwrap();
        let bridge_cafile = ca_file.path().to_str().unwrap().to_owned();

        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let key_file = NamedTempFile::new().unwrap();
        let bridge_keyfile = key_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_cafile,
            bridge_certfile,
            bridge_keyfile,
            ..C8yConfig::default()
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_c8y_validate_wrong_url() {
        let config = Config::C8y(C8yConfig {
            address: INCORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_cert_path() {
        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_c8y_validate_wrong_key_path() {
        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let config = Config::C8y(C8yConfig {
            address: CORRECT_URL.into(),
            bridge_certfile,
            bridge_keyfile: INCORRECT_PATH.into(),
            ..C8yConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn bridge_config_c8y_create() {
        let mut bridge = C8yConfig::default();

        bridge.bridge_cafile = "./test_root.pem".into();
        bridge.address = "test.test.io:8883".into();
        bridge.bridge_certfile = "./test-certificate.pem".into();
        bridge.bridge_keyfile = "./test-private-key.pem".into();

        let expected = C8yConfig {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            connection: "edge_to_c8y".into(),
            remote_clientid: "alpha".into(),
            try_private: false,
            start_type: "automatic".into(),
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
        };

        assert_eq!(bridge, expected);
    }
}
*/
