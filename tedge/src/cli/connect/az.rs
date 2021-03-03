use super::*;
use crate::config::ConfigError;
use async_trait::async_trait;
const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";

pub struct Azure {}

impl Azure {
    pub fn azure_bridge_config() -> Result<BridgeConfig, ConfigError> {
        let config = TEdgeConfig::from_default_config()?;
        let az_url = get_config_value(&config, _AZURE_URL)?;
        let clientid = get_config_value(&config, DEVICE_ID)?;
        let iothub_name: Vec<&str> = az_url.split(":").collect();
        let user_name = format!("{}", iothub_name.into_iter().nth(0).unwrap())
            + "/"
            + &clientid.to_string()
            + "/?api-version=2018-06-30";
        let pub_msg_topic = format!("messages/events/ out 1 az/ devices/{}/", clientid);
        let sub_msg_topic = format!("messages/devicebound/# out 1 az/ devices/{}/", clientid);

        Ok(BridgeConfig {
            cloud_name: "az".into(),
            config_file: AZURE_CONFIG_FILENAME.to_string(),
            connection: "edge_to_az".into(),
            address: az_url,
            remote_username: user_name,
            bridge_cafile: get_config_value(&config, _AZURE_ROOT_CERT_PATH)?,
            remote_clientid: clientid,
            local_clientid: "Azure".into(),
            bridge_certfile: get_config_value(&config, DEVICE_CERT_PATH)?,
            bridge_keyfile: get_config_value(&config, DEVICE_KEY_PATH)?,
            try_private: false,
            start_type: "automatic".into(),
            cleansession: true,
            bridge_insecure: false,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                r##"$iothub/twin/res/# in 1"##.into(),
                r#"$iothub/twin/GET/?$rid=1 out 1"#.into(),
            ],
            cloud_connect: "azure.connect".into(),
            check_connection: Box::new(Azure {}),
        })
    }
}

#[async_trait]
impl CheckConnection for Azure {
    async fn check_connection(&self) -> Result<(), ConnectError> {
        Ok(())
    }
}
