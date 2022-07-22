use crate::cli::connect::BridgeConfig;
use tedge_config::{ConnectUrl, FilePath};

#[derive(Debug, PartialEq)]
pub struct BridgeConfigAwsParams {
    pub connect_url: ConnectUrl,
    pub mqtt_tls_port: u16,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: FilePath,
    pub bridge_certfile: FilePath,
    pub bridge_keyfile: FilePath,
}

impl From<BridgeConfigAwsParams> for BridgeConfig {
    fn from(params: BridgeConfigAwsParams) -> Self {
        let BridgeConfigAwsParams {
            connect_url,
            mqtt_tls_port,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
        } = params;

        let address = format!("{}:{}", connect_url.as_str(), mqtt_tls_port);
        let user_name = format!(
            "{}/{}/?api-version=2018-06-30",
            connect_url.as_str(),
            remote_clientid
        );
        Self {
            cloud_name: "aws".into(),
            config_file,
            connection: "edge_to_aws".into(),
            address,
            remote_username: Some(user_name),
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: "AWS".into(),
            bridge_certfile,
            bridge_keyfile,
            use_mapper: false,
            use_agent: false,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: false,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "tedge/health/mosquitto-aws-bridge".into(),
            bridge_attempt_unsubscribe: false,
            topics: vec![
                // let's just share all the topics both ways for now
                // TODO: proper AWS reserved topic mappings
                // https://docs.aws.amazon.com/iot/latest/developerguide/topics.html
                r"things/# both 1 aws/ $aws/".into(),
                r"tedge/measurements/# both 1".into(),
            ],
        }
    }
}
