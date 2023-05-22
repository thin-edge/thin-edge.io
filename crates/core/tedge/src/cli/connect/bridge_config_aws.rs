use crate::cli::connect::BridgeConfig;
use camino::Utf8PathBuf;
use tedge_config::ConnectUrl;

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfigAwsParams {
    pub connect_url: ConnectUrl,
    pub mqtt_tls_port: u16,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
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

        let address = format!("{}:{}", connect_url, mqtt_tls_port);
        let user_name = remote_clientid.to_string();

        // telemetry/command topics for use by the user
        let pub_msg_topic = format!("td/# out 1 aws/ thinedge/{remote_clientid}/");
        let sub_msg_topic = format!("cmd/# in 1 aws/ thinedge/{remote_clientid}/");

        // topic to interact with the shadow of the device
        let shadow_topic = format!("shadow/# both 1 aws/ $aws/things/{remote_clientid}/");

        // echo topic mapping to check the connection
        let connection_check_pub_msg_topic = format!(
            r#""" out 1 aws/test-connection thinedge/devices/{remote_clientid}/test-connection"#
        );
        let connection_check_sub_msg_topic = format!(
            r#""" in 1 aws/connection-success thinedge/devices/{remote_clientid}/test-connection"#
        );

        Self {
            cloud_name: "aws".into(),
            config_file,
            connection: "edge_to_aws".into(),
            address,
            remote_username: Some(user_name),
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: "Aws".into(),
            bridge_certfile,
            bridge_keyfile,
            use_mapper: true,
            use_agent: false,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "tedge/health/mosquitto-aws-bridge".into(),
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                shadow_topic,
                connection_check_pub_msg_topic,
                connection_check_sub_msg_topic,
            ],
        }
    }
}

#[test]
fn test_bridge_config_from_aws_params() -> anyhow::Result<()> {
    use std::convert::TryFrom;

    let params = BridgeConfigAwsParams {
        connect_url: ConnectUrl::try_from("test.test.io")?,
        mqtt_tls_port: 8883,
        config_file: "aws-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "aws".into(),
        config_file: "aws-bridge.conf".to_string(),
        connection: "edge_to_aws".into(),
        address: "test.test.io:8883".into(),
        remote_username: Some("alpha".into()),
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Aws".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: false,
        topics: vec![
            "td/# out 1 aws/ thinedge/alpha/".into(),
            "cmd/# in 1 aws/ thinedge/alpha/".into(),
            "shadow/# both 1 aws/ $aws/things/alpha/".into(),
            r#""" out 1 aws/test-connection thinedge/devices/alpha/test-connection"#.into(),
            r#""" in 1 aws/connection-success thinedge/devices/alpha/test-connection"#.into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: "tedge/health/mosquitto-aws-bridge".into(),
        bridge_attempt_unsubscribe: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
