use crate::cli::connect::BridgeConfig;
use camino::Utf8PathBuf;
use tedge_config::ConnectUrl;

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfigAzureParams {
    pub connect_url: ConnectUrl,
    pub mqtt_tls_port: u16,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
}

impl From<BridgeConfigAzureParams> for BridgeConfig {
    fn from(params: BridgeConfigAzureParams) -> Self {
        let BridgeConfigAzureParams {
            connect_url,
            mqtt_tls_port,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
        } = params;

        let address = format!("{}:{}", connect_url, mqtt_tls_port);
        let user_name = format!(
            "{}/{}/?api-version=2018-06-30",
            connect_url, remote_clientid
        );
        let pub_msg_topic = format!("messages/events/# out 1 az/ devices/{}/", remote_clientid);
        let sub_msg_topic = format!(
            "messages/devicebound/# out 1 az/ devices/{}/",
            remote_clientid
        );
        Self {
            cloud_name: "az".into(),
            config_file,
            connection: "edge_to_az".into(),
            address,
            remote_username: Some(user_name),
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: "Azure".into(),
            bridge_certfile,
            bridge_keyfile,
            use_mapper: true,
            use_agent: false,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "tedge/health/mosquitto-az-bridge".into(),
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                r##"twin/res/# in 1 az/ $iothub/"##.into(),
                r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
            ],
        }
    }
}

#[test]
fn test_bridge_config_from_azure_params() -> anyhow::Result<()> {
    use std::convert::TryFrom;

    let params = BridgeConfigAzureParams {
        connect_url: ConnectUrl::try_from("test.test.io")?,
        mqtt_tls_port: 8883,
        config_file: "az-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "az".into(),
        config_file: "az-bridge.conf".to_string(),
        connection: "edge_to_az".into(),
        address: "test.test.io:8883".into(),
        remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Azure".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: false,
        topics: vec![
            r#"messages/events/# out 1 az/ devices/alpha/"#.into(),
            r##"messages/devicebound/# out 1 az/ devices/alpha/"##.into(),
            r##"twin/res/# in 1 az/ $iothub/"##.into(),
            r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: "tedge/health/mosquitto-az-bridge".into(),
        bridge_attempt_unsubscribe: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
