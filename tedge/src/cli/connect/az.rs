use super::*;
//use mqtt_client::{Client, Message, Topic, TopicFilter};
pub struct Azure {

}

impl Azure {
   pub fn get_azure_topics(config: &BridgeConfig)->Vec<String> {
        let pub_msg_topic =  format!("messages/events/ out 1 az/ devices/{}/",config.remote_clientid);
        let sub_msg_topic =  format!("messages/devicebound/# out 1 az/ devices/{}/",config.remote_clientid);
        let topics = vec![
            pub_msg_topic,
            sub_msg_topic,
            r##"$iothub/twin/res/# in 1"##.into(),
            r#"$iothub/twin/GET/?$rid=1 out 1"#.into(),
        ];
        topics
    }

// Here We check the az device twin properties over mqtt to check if connection has been open.
    // First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
    // device twin property output
    // Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID
    // The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}
    // Here if the status is 200 then its success
    
   /*
    #[tokio::main]
    async fn check_connection() -> Result<(), ConnectError> {

        const AZURE_TOPIC_DEVICETWIN_DOWNSTREAM: &str = r##"$iothub/twin/res/#"##;
        const AZURE_TOPIC_DEVICETWIN_UPSTREAM: &str = r#"$iothub/twin/GET/?$rid=1"#;
        const CLIENT_ID: &str = "check_connection";

        let template_pub_topic = Topic::new("$iothub/twin/GET/?$rid=1")?;
        let template_sub_filter = TopicFilter::new("$iothub/twin/res/200/?$rid=1")?;

        let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
        let mut device_twin_response = mqtt.subscribe(template_sub_filter).await?;

        //let (sender, receiver) = tokio::sync::oneshot::channel();

/*
        let _task_handle = tokio::spawn(async move {
            while let Some(message) = device_twin_response.next().await {
                println!("msg====>{:#?}",message);
                let _ = sender.send(true);
                break;

                if std::str::from_utf8(message.topic)
                    .unwrap_or("")
                    .contains("200")
                {
                    let _ = sender.send(true);
                    break;
                }
            }
        });

*/
        mqtt.publish(Message::new(&template_pub_topic, "")).await?;

        println!("-------waiting for response---------------");
                
        if let Some(message) = device_twin_response.next().await{
                println!("msg====>{:#?}",message);
        }
        /*
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received message.");
            }
            _err => {
                return Err(ConnectError::BridgeConnectionFailed {cloud: String::from("azure")});
            }
        }
        */

        Ok(())
    }
*/


}

/*
#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";

    #[test]
    fn config_az_validate_ok() {
        let ca_file = NamedTempFile::new().unwrap();
        let bridge_cafile = ca_file.path().to_str().unwrap().to_owned();

        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let key_file = NamedTempFile::new().unwrap();
        let bridge_keyfile = key_file.path().to_str().unwrap().to_owned();

        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_cafile,
            bridge_certfile,
            bridge_keyfile,
            ..AzConfig::default()
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_az_validate_wrong_url() {
        let config = Config::AZURE(AzConfig {
            address: INCORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_az_validate_wrong_cert_path() {
        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_az_validate_wrong_key_path() {
        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_certfile,
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[ignore]
    fn bridge_config_az_create() {
        let mut bridge = AzConfig::default();

        bridge.bridge_cafile = "./test_root.pem".into();
        bridge.address = "test.test.io:8883".into();
        bridge.bridge_certfile = "./test-certificate.pem".into();
        bridge.bridge_keyfile = "./test-private-key.pem".into();

        let expected = AzConfig {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            remote_username: r#"test.test.io:8883".into()/"alpha".into()/?api-version=2018-06-30"#.into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            connection: "edge_to_az".into(),
            remote_clientid: "alpha".into(),
            try_private: false,
            start_type: "automatic".into(),
            topics: vec![
                // az JSON
                r#"messages/events/ out 1 az/ """#.into(),
            ],
        };

        assert_eq!(bridge, expected);
    }
}
*/
