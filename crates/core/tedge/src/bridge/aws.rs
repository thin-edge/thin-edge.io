use super::BridgeConfig;
use crate::bridge::config::BridgeLocation;
use camino::Utf8PathBuf;
use std::borrow::Cow;
use std::time::Duration;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::HostPort;
use tedge_config::ProfileName;
use tedge_config::TopicPrefix;
use tedge_config::MQTT_TLS_PORT;

#[derive(Debug)]
pub struct BridgeConfigAwsParams {
    pub mqtt_host: HostPort<MQTT_TLS_PORT>,
    pub config_file: Cow<'static, str>,
    pub remote_clientid: String,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub bridge_location: BridgeLocation,
    pub topic_prefix: TopicPrefix,
    pub profile_name: Option<ProfileName>,
    pub mqtt_schema: MqttSchema,
    pub keepalive_interval: Duration,
}

impl From<BridgeConfigAwsParams> for BridgeConfig {
    fn from(params: BridgeConfigAwsParams) -> Self {
        let BridgeConfigAwsParams {
            mqtt_host,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            bridge_location,
            topic_prefix,
            profile_name,
            mqtt_schema,
            keepalive_interval,
        } = params;

        let user_name = remote_clientid.to_string();

        // telemetry/command topics for use by the user
        let pub_msg_topic = format!("td/# out 1 {topic_prefix}/ thinedge/{remote_clientid}/");
        let sub_msg_topic = format!("cmd/# in 1 {topic_prefix}/ thinedge/{remote_clientid}/");

        // topic to interact with the shadow of the device
        let shadow_topic =
            format!("shadow/# both 1 {topic_prefix}/ $aws/things/{remote_clientid}/");

        // echo topic mapping to check the connection
        let connection_check_pub_msg_topic = format!(
            r#""" out 1 {topic_prefix}/test-connection thinedge/devices/{remote_clientid}/test-connection"#
        );
        let connection_check_sub_msg_topic = format!(
            r#""" in 1 {topic_prefix}/connection-success thinedge/devices/{remote_clientid}/test-connection"#
        );

        let service_name = format!("mosquitto-{topic_prefix}-bridge");
        let health = mqtt_schema.topic_for(
            &EntityTopicId::default_main_service(&service_name).unwrap(),
            &Channel::Health,
        );
        Self {
            cloud_name: "aws".into(),
            config_file,
            connection: if let Some(profile) = &profile_name {
                format!("edge_to_aws@{profile}")
            } else {
                "edge_to_aws".into()
            },
            address: mqtt_host,
            remote_username: Some(user_name),
            remote_password: None,
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: if let Some(profile) = &profile_name {
                format!("Aws@{profile}")
            } else {
                "Aws".into()
            },
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
            notification_topic: health.name,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                shadow_topic,
                connection_check_pub_msg_topic,
                connection_check_sub_msg_topic,
            ],
            bridge_location,
            // AWS IoT Just In Time Provisioning (JITP) uses the first connection
            // to create the "Thing", so the first connection attempt can fail, but retrying
            // will give it a higher chance of success
            connection_check_attempts: 5,
            auth_method: None,
            mosquitto_version: None,
            keepalive_interval,
            use_cryptoki: false,
        }
    }
}

#[test]
fn test_bridge_config_from_aws_params() -> anyhow::Result<()> {
    let params = BridgeConfigAwsParams {
        mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        config_file: "aws-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        bridge_location: BridgeLocation::Mosquitto,
        topic_prefix: "aws".try_into().unwrap(),
        profile_name: None,
        mqtt_schema: MqttSchema::with_root("te".into()),
        keepalive_interval: Duration::from_secs(60),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "aws".into(),
        config_file: "aws-bridge.conf".into(),
        connection: "edge_to_aws".into(),
        address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        remote_username: Some("alpha".into()),
        remote_password: None,
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
        include_local_clean_session: false,
        local_clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: "te/device/main/service/mosquitto-aws-bridge/status/health".into(),
        bridge_attempt_unsubscribe: false,
        bridge_location: BridgeLocation::Mosquitto,
        connection_check_attempts: 5,
        auth_method: None,
        mosquitto_version: None,
        keepalive_interval: Duration::from_secs(60),
        use_cryptoki: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}

#[test]
fn test_bridge_config_aws_custom_topic_prefix() -> anyhow::Result<()> {
    let params = BridgeConfigAwsParams {
        mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        config_file: "aws-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        bridge_location: BridgeLocation::Mosquitto,
        topic_prefix: "aws-custom".try_into().unwrap(),
        profile_name: Some("profile".parse().unwrap()),
        mqtt_schema: MqttSchema::with_root("te".into()),
        keepalive_interval: Duration::from_secs(60),
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "aws".into(),
        config_file: "aws-bridge.conf".into(),
        connection: "edge_to_aws@profile".into(),
        address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
        remote_username: Some("alpha".into()),
        remote_password: None,
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Aws@profile".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: false,
        topics: vec![
            "td/# out 1 aws-custom/ thinedge/alpha/".into(),
            "cmd/# in 1 aws-custom/ thinedge/alpha/".into(),
            "shadow/# both 1 aws-custom/ $aws/things/alpha/".into(),
            r#""" out 1 aws-custom/test-connection thinedge/devices/alpha/test-connection"#.into(),
            r#""" in 1 aws-custom/connection-success thinedge/devices/alpha/test-connection"#
                .into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: false,
        include_local_clean_session: false,
        local_clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: "te/device/main/service/mosquitto-aws-custom-bridge/status/health"
            .into(),
        bridge_attempt_unsubscribe: false,
        bridge_location: BridgeLocation::Mosquitto,
        connection_check_attempts: 5,
        auth_method: None,
        mosquitto_version: None,
        keepalive_interval: Duration::from_secs(60),
        use_cryptoki: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
