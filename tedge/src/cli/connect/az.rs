use super::*;
use crate::config::ConfigError;
use crate::utils::config;
use mqtt_client::{Client, Message, Topic, TopicFilter};
use std::time::Duration;
use tokio::time::timeout;

const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Azure {}

impl Azure {
    pub fn azure_bridge_config(config: TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        let az_url =
            config::parse_user_provided_address(config::get_config_value(&config, AZURE_URL)?)?;
        let address = format!("{}:{}", az_url, MQTT_TLS_PORT);
        let clientid = config::get_config_value(&config, DEVICE_ID)?;
        let user_name = format!("{}/{}/?api-version=2018-06-30", az_url, &clientid);
        let pub_msg_topic = format!("messages/events/ out 1 az/ devices/{}/", clientid);
        let sub_msg_topic = format!("messages/devicebound/# out 1 az/ devices/{}/", clientid);

        Ok(BridgeConfig {
            cloud_name: "az".into(),
            config_file: AZURE_CONFIG_FILENAME.to_string(),
            connection: "edge_to_az".into(),
            address,
            remote_username: Some(user_name),
            bridge_cafile: config::get_config_value(&config, AZURE_ROOT_CERT_PATH)?,
            remote_clientid: clientid,
            local_clientid: "Azure".into(),
            bridge_certfile: config::get_config_value(&config, DEVICE_CERT_PATH)?,
            bridge_keyfile: config::get_config_value(&config, DEVICE_KEY_PATH)?,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                r##"twin/res/# in 1 az/ $iothub/"##.into(),
                r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
            ],
            ..BridgeConfig::default()
        })
    }

    // Here We check the az device twin properties over mqtt to check if connection has been open.
    // First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
    // device twin property output.
    // Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID.
    // The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}.
    // Here if the status is 200 then it's success.

    #[tokio::main]
    async fn check_connection_async(&self) -> Result<(), ConnectError> {
        const AZURE_TOPIC_DEVICETWIN_DOWNSTREAM: &str = r##"az/twin/res/#"##;
        const AZURE_TOPIC_DEVICETWIN_UPSTREAM: &str = r#"az/twin/GET/?$rid=1"#;
        const CLIENT_ID: &str = "check_connection_az";

        let template_pub_topic = Topic::new(AZURE_TOPIC_DEVICETWIN_UPSTREAM)?;
        let template_sub_filter = TopicFilter::new(AZURE_TOPIC_DEVICETWIN_DOWNSTREAM)?;

        let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
        let mut device_twin_response = mqtt.subscribe(template_sub_filter).await?;

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

        mqtt.publish(Message::new(&template_pub_topic, "".to_string()))
            .await?;

        let fut = timeout(RESPONSE_TIMEOUT, &mut receiver);
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received expected response message, connection check is successful");
                return Ok(());
            }
            _err => {
                println!("Warning: No response, bridge has been configured, but Azure connection check failed.\n",);
                Ok(())
            }
        }
    }
}

impl CheckConnection for Azure {
    fn check_connection(&self) -> Result<(), ConnectError> {
        Ok(self.check_connection_async()?)
    }
}
