use super::*;
use crate::config::ConfigError;
use crate::utils::config;

const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";

pub struct Azure {}

impl Azure {
    pub fn azure_bridge_config(config: TEdgeConfig) -> Result<BridgeConfig, ConfigError> {
        let az_url = config::get_config_value(&config, AZURE_URL)?;
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
            try_private: false,
            start_type: "automatic".into(),
            cleansession: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                r##"twin/res/# in 1 az/ $iothub/"##.into(),
                r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
            ],
        })
    }
}

impl CheckConnection for Azure {
    fn check_connection(&self) -> Result<(), ConnectError> {
        Ok(())
    }
}
