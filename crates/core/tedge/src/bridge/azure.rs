use super::BridgeConfig;
use crate::bridge::config::BridgeLocation;
use camino::Utf8PathBuf;
use tedge_config::HostPort;
use tedge_config::TopicPrefix;
use tedge_config::MQTT_TLS_PORT;

const MOSQUITTO_BRIDGE_TOPIC: &str = "te/device/main/service/mosquitto-az-bridge/status/health";

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfigAzureParams {
    pub mqtt_host: HostPort<MQTT_TLS_PORT>,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub bridge_location: BridgeLocation,
    pub topic_prefix: TopicPrefix,
}

impl From<BridgeConfigAzureParams> for BridgeConfig {
    fn from(params: BridgeConfigAzureParams) -> Self {
        let BridgeConfigAzureParams {
            mqtt_host,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            bridge_location,
            topic_prefix,
        } = params;

        let address = mqtt_host.clone();
        let user_name = format!(
            "{}/{}/?api-version=2018-06-30",
            mqtt_host.host(),
            remote_clientid
        );
        let pub_msg_topic =
            format!("messages/events/# out 1 {topic_prefix}/ devices/{remote_clientid}/");
        let sub_msg_topic =
            format!("messages/devicebound/# in 1 {topic_prefix}/ devices/{remote_clientid}/");
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
            include_local_clean_session: false, // local_clean_session being equal to clean_session, the former is useless and safer to ignore
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: MOSQUITTO_BRIDGE_TOPIC.into(),
            bridge_attempt_unsubscribe: false,
            topics: vec![
                // See Azure IoT Hub documentation for detailed explanation on the topics
                // https://learn.microsoft.com/en-us/azure/iot/iot-mqtt-connect-to-iot-hub#receiving-cloud-to-device-messages
                pub_msg_topic,
                sub_msg_topic,
                // Direct methods (request/response)
                format!("methods/POST/# in 1 {topic_prefix}/ $iothub/"),
                format!("methods/res/# out 1 {topic_prefix}/ $iothub/"),
                // Digital twin
                format!("twin/res/# in 1 {topic_prefix}/ $iothub/"),
                format!("twin/GET/# out 1 {topic_prefix}/ $iothub/"),
                format!("twin/PATCH/# out 1 {topic_prefix}/ $iothub/"),
            ],
            bridge_location,
            connection_check_attempts: 1,
        }
    }
}

#[test]
fn test_bridge_config_from_azure_params() -> anyhow::Result<()> {
    use std::convert::TryFrom;

    let params = BridgeConfigAzureParams {
        mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        config_file: "az-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        bridge_location: BridgeLocation::Mosquitto,
        topic_prefix: "az".try_into().unwrap(),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "az".into(),
        config_file: "az-bridge.conf".to_string(),
        connection: "edge_to_az".into(),
        address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Azure".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: false,
        topics: vec![
            "messages/events/# out 1 az/ devices/alpha/".into(),
            "messages/devicebound/# in 1 az/ devices/alpha/".into(),
            "methods/POST/# in 1 az/ $iothub/".into(),
            "methods/res/# out 1 az/ $iothub/".into(),
            "twin/res/# in 1 az/ $iothub/".into(),
            "twin/GET/# out 1 az/ $iothub/".into(),
            "twin/PATCH/# out 1 az/ $iothub/".into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: false,
        include_local_clean_session: false,
        local_clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: MOSQUITTO_BRIDGE_TOPIC.into(),
        bridge_attempt_unsubscribe: false,
        bridge_location: BridgeLocation::Mosquitto,
        connection_check_attempts: 1,
    };

    assert_eq!(bridge, expected);

    Ok(())
}

#[test]
fn test_azure_bridge_config_with_custom_prefix() -> anyhow::Result<()> {
    use std::convert::TryFrom;

    let params = BridgeConfigAzureParams {
        mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        config_file: "az-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        bridge_location: BridgeLocation::Mosquitto,
        topic_prefix: "custom".try_into().unwrap(),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "az".into(),
        config_file: "az-bridge.conf".to_string(),
        connection: "edge_to_az".into(),
        address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Azure".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: false,
        topics: vec![
            "messages/events/# out 1 custom/ devices/alpha/".into(),
            "messages/devicebound/# in 1 custom/ devices/alpha/".into(),
            "methods/POST/# in 1 custom/ $iothub/".into(),
            "methods/res/# out 1 custom/ $iothub/".into(),
            "twin/res/# in 1 custom/ $iothub/".into(),
            "twin/GET/# out 1 custom/ $iothub/".into(),
            "twin/PATCH/# out 1 custom/ $iothub/".into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: false,
        include_local_clean_session: false,
        local_clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: MOSQUITTO_BRIDGE_TOPIC.into(),
        bridge_attempt_unsubscribe: false,
        bridge_location: BridgeLocation::Mosquitto,
        connection_check_attempts: 1,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
