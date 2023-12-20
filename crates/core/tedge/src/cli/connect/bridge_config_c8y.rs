use crate::cli::connect::BridgeConfig;
use camino::Utf8PathBuf;
use std::process::Command;
use tedge_config::AutoFlag;
use tedge_config::HostPort;
use tedge_config::TemplatesSet;
use tedge_config::MQTT_TLS_PORT;
use which::which;

const C8Y_BRIDGE_HEALTH_TOPIC: &str = "te/device/main/service/mosquitto-c8y-bridge/status/health";

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfigC8yParams {
    pub mqtt_host: HostPort<MQTT_TLS_PORT>,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub smartrest_templates: TemplatesSet,
    pub include_local_clean_session: AutoFlag,
}

impl From<BridgeConfigC8yParams> for BridgeConfig {
    fn from(params: BridgeConfigC8yParams) -> Self {
        let BridgeConfigC8yParams {
            mqtt_host,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            smartrest_templates,
            include_local_clean_session,
        } = params;

        let mut topics: Vec<String> = vec![
            // Templates
            r#"s/dt in 2 c8y/ """#.into(),
            r#"s/ut/# out 2 c8y/ """#.into(),
            // Static templates
            r#"s/us/# out 2 c8y/ """#.into(),
            r#"t/us/# out 2 c8y/ """#.into(),
            r#"q/us/# out 2 c8y/ """#.into(),
            r#"c/us/# out 2 c8y/ """#.into(),
            r#"s/ds in 2 c8y/ """#.into(),
            // Debug
            r#"s/e in 0 c8y/ """#.into(),
            // SmartRest2
            r#"s/uc/# out 2 c8y/ """#.into(),
            r#"t/uc/# out 2 c8y/ """#.into(),
            r#"q/uc/# out 2 c8y/ """#.into(),
            r#"c/uc/# out 2 c8y/ """#.into(),
            r#"s/dc/# in 2 c8y/ """#.into(),
            // c8y JSON
            r#"inventory/managedObjects/update/# out 2 c8y/ """#.into(),
            r#"measurement/measurements/create out 2 c8y/ """#.into(),
            r#"event/events/create out 2 c8y/ """#.into(),
            r#"alarm/alarms/create out 2 c8y/ """#.into(),
            r#"error in 2 c8y/ """#.into(),
            // c8y JWT token retrieval
            r#"s/uat out 0 c8y/ """#.into(),
            r#"s/dat in 0 c8y/ """#.into(),
        ];

        let templates_set = smartrest_templates
            .0
            .iter()
            .flat_map(|s| {
                // Smartrest templates should be deserialized as:
                // c8y/s/uc/template-1 (in from localhost), s/uc/template-1
                // c8y/s/dc/template-1 (out to localhost), s/dc/template-1
                [
                    format!(r#"s/uc/{s} out 2 c8y/ """#),
                    format!(r#"s/dc/{s} in 2 c8y/ """#),
                ]
                .into_iter()
            })
            .collect::<Vec<String>>();
        topics.extend(templates_set);

        let include_local_clean_session = match include_local_clean_session {
            AutoFlag::True => true,
            AutoFlag::False => false,
            AutoFlag::Auto => is_mosquitto_version_above_2(),
        };

        Self {
            cloud_name: "c8y".into(),
            config_file,
            connection: "edge_to_c8y".into(),
            address: format!(
                "{host}:{port}",
                host = mqtt_host.host(),
                port = mqtt_host.port().0
            ),
            remote_username: None,
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: "Cumulocity".into(),
            bridge_certfile,
            bridge_keyfile,
            use_mapper: true,
            use_agent: true,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session,
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,

            // FIXME: doesn't account for custom topic root, use MQTT scheme API here
            notification_topic: C8Y_BRIDGE_HEALTH_TOPIC.into(),
            bridge_attempt_unsubscribe: false,
            topics,
        }
    }
}

/// Return whether or not mosquitto version is >= 2.0.0
///
/// As mosquitto doesn't provide a `--version` flag, this is a guess.
///
/// The main requirement is to ensure there is no false positive,
/// as this used to generate configuration files
/// with recent mosquitto properties (such as `local_cleansession`)
/// which make crash old versions (< 2.0.0).
pub fn is_mosquitto_version_above_2() -> bool {
    if let Ok(mosquitto) = which("mosquitto") {
        if let Ok(mosquitto_help) = Command::new(mosquitto).args(["--help"]).output() {
            if let Ok(help_content) = String::from_utf8(mosquitto_help.stdout) {
                let is_above_2 = help_content.starts_with("mosquitto version 2");
                if is_above_2 {
                    eprintln!("Detected mosquitto version >= 2.0.0");
                } else {
                    eprintln!("Detected mosquitto version < 2.0.0");
                }
                return is_above_2;
            }
        }
    }

    eprintln!("Failed to detect mosquitto version: assuming mosquitto version < 2.0.0");
    false
}

#[test]
fn test_bridge_config_from_c8y_params() -> anyhow::Result<()> {
    use std::convert::TryFrom;
    let params = BridgeConfigC8yParams {
        mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io".to_string())?,
        config_file: "c8y-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        smartrest_templates: TemplatesSet::try_from(vec!["abc", "def"])?,
        include_local_clean_session: AutoFlag::False,
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "c8y".into(),
        config_file: "c8y-bridge.conf".into(),
        connection: "edge_to_c8y".into(),
        address: "test.test.io:8883".into(),
        remote_username: None,
        bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
        remote_clientid: "alpha".into(),
        local_clientid: "Cumulocity".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: true,
        topics: vec![
            // Templates
            r#"s/dt in 2 c8y/ """#.into(),
            r#"s/ut/# out 2 c8y/ """#.into(),
            // Static templates
            r#"s/us/# out 2 c8y/ """#.into(),
            r#"t/us/# out 2 c8y/ """#.into(),
            r#"q/us/# out 2 c8y/ """#.into(),
            r#"c/us/# out 2 c8y/ """#.into(),
            r#"s/ds in 2 c8y/ """#.into(),
            // Debug
            r#"s/e in 0 c8y/ """#.into(),
            // SmartRest2
            r#"s/uc/# out 2 c8y/ """#.into(),
            r#"t/uc/# out 2 c8y/ """#.into(),
            r#"q/uc/# out 2 c8y/ """#.into(),
            r#"c/uc/# out 2 c8y/ """#.into(),
            r#"s/dc/# in 2 c8y/ """#.into(),
            // c8y JSON
            r#"inventory/managedObjects/update/# out 2 c8y/ """#.into(),
            r#"measurement/measurements/create out 2 c8y/ """#.into(),
            r#"event/events/create out 2 c8y/ """#.into(),
            r#"alarm/alarms/create out 2 c8y/ """#.into(),
            r#"error in 2 c8y/ """#.into(),
            // c8y JWT token retrieval
            r#"s/uat out 0 c8y/ """#.into(),
            r#"s/dat in 0 c8y/ """#.into(),
            // Smartrest templates should be deserialized as:
            // s/uc/template-1 (in from localhost), s/uc/template-1
            // s/dc/template-1 (out to localhost), s/dc/template-1
            r#"s/uc/abc out 2 c8y/ """#.into(),
            r#"s/dc/abc in 2 c8y/ """#.into(),
            r#"s/uc/def out 2 c8y/ """#.into(),
            r#"s/dc/def in 2 c8y/ """#.into(),
        ],
        try_private: false,
        start_type: "automatic".into(),
        clean_session: true,
        include_local_clean_session: false,
        local_clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: C8Y_BRIDGE_HEALTH_TOPIC.into(),
        bridge_attempt_unsubscribe: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
