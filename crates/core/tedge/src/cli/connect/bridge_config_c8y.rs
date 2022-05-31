use crate::cli::connect::BridgeConfig;
use tedge_config::{ConnectUrl, FilePath, TemplatesSet};

#[derive(Debug, PartialEq)]
pub struct BridgeConfigC8yParams {
    pub connect_url: ConnectUrl,
    pub mqtt_tls_port: u16,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: FilePath,
    pub bridge_certfile: FilePath,
    pub bridge_keyfile: FilePath,
    pub smartrest_templates: TemplatesSet,
}

impl From<BridgeConfigC8yParams> for BridgeConfig {
    fn from(params: BridgeConfigC8yParams) -> Self {
        let BridgeConfigC8yParams {
            connect_url,
            mqtt_tls_port,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            smartrest_templates,
        } = params;
        let address = format!("{}:{}", connect_url.as_str(), mqtt_tls_port);

        let mut topics: Vec<String> = vec![
            // Registration
            r#"s/dcr in 2 c8y/ """#.into(),
            r#"s/ucr out 2 c8y/ """#.into(),
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
            r#"error in 2 c8y/ """#.into(),
            // c8y JWT token retrieval
            r#"s/uat/# out 2 c8y/ """#.into(),
            r#"s/dat/# in 2 c8y/ """#.into(),
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

        Self {
            cloud_name: "c8y".into(),
            config_file,
            connection: "edge_to_c8y".into(),
            address,
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
            clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "tedge/health/mosquitto-c8y-bridge".into(),
            bridge_attempt_unsubscribe: false,
            topics,
        }
    }
}

#[test]
fn test_bridge_config_from_c8y_params() -> anyhow::Result<()> {
    use std::convert::TryFrom;
    let params = BridgeConfigC8yParams {
        connect_url: ConnectUrl::try_from("test.test.io")?,
        mqtt_tls_port: 8883,
        config_file: "c8y-bridge.conf".into(),
        remote_clientid: "alpha".into(),
        bridge_root_cert_path: "./test_root.pem".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        smartrest_templates: TemplatesSet::try_from(vec!["abc", "def"])?,
    };

    let bridge = BridgeConfig::from(params);

    let expected = BridgeConfig {
        cloud_name: "c8y".into(),
        config_file: "c8y-bridge.conf".into(),
        connection: "edge_to_c8y".into(),
        address: "test.test.io:8883".into(),
        remote_username: None,
        bridge_root_cert_path: "./test_root.pem".into(),
        remote_clientid: "alpha".into(),
        local_clientid: "Cumulocity".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        use_mapper: true,
        use_agent: true,
        topics: vec![
            // Registration
            r#"s/dcr in 2 c8y/ """#.into(),
            r#"s/ucr out 2 c8y/ """#.into(),
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
            r#"error in 2 c8y/ """#.into(),
            // c8y JWT token retrieval
            r#"s/uat/# out 2 c8y/ """#.into(),
            r#"s/dat/# in 2 c8y/ """#.into(),
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
        clean_session: false,
        notifications: true,
        notifications_local_only: true,
        notification_topic: "tedge/health/mosquitto-c8y-bridge".into(),
        bridge_attempt_unsubscribe: false,
    };

    assert_eq!(bridge, expected);

    Ok(())
}
