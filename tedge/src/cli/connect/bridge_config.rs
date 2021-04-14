use crate::cli::connect::COMMON_MOSQUITTO_CONFIG_FILENAME;

#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
    pub common_mosquitto_config: CommonMosquittoConfig,
    pub cloud_name: String,
    pub config_file: String,
    pub connection: String,
    pub address: String,
    pub remote_username: Option<String>,
    pub bridge_root_cert_path: String,
    pub remote_clientid: String,
    pub local_clientid: String,
    pub bridge_certfile: String,
    pub bridge_keyfile: String,
    pub use_mapper: bool,
    pub try_private: bool,
    pub start_type: String,
    pub clean_session: bool,
    pub notifications: bool,
    pub bridge_attempt_unsubscribe: bool,
    pub topics: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct CommonMosquittoConfig {
    pub config_file: String,
    pub listener: String,
    pub allow_anonymous: bool,
    pub connection_messages: bool,
    pub log_types: Vec<String>,
}

impl Default for CommonMosquittoConfig {
    fn default() -> Self {
        CommonMosquittoConfig {
            config_file: COMMON_MOSQUITTO_CONFIG_FILENAME.into(),
            listener: "1883 localhost".into(),
            allow_anonymous: true,
            connection_messages: true,
            log_types: vec![
                "error".into(),
                "warning".into(),
                "notice".into(),
                "information".into(),
                "subscribe".into(),
                "unsubscribe".into(),
            ],
        }
    }
}
