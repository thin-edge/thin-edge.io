use super::config::ProxyWrapper;
use super::BridgeConfig;
use crate::bridge::config::BridgeLocation;
use crate::ConfigError;
use c8y_api::http_proxy::read_c8y_credentials;
use camino::Utf8PathBuf;
use std::borrow::Cow;
use std::process::Command;
use std::time::Duration;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::auth_method::AuthType;
use tedge_config::models::AutoFlag;
use tedge_config::models::HostPort;
use tedge_config::models::TemplatesSet;
use tedge_config::models::TopicPrefix;
use tedge_config::models::MQTT_SVC_TLS_PORT;
use tedge_config::models::MQTT_TLS_PORT;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use which::which;

#[derive(Debug)]
pub struct BridgeConfigC8yParams {
    pub mqtt_host: HostPort<MQTT_TLS_PORT>,
    pub config_file: Cow<'static, str>,
    pub remote_clientid: String,
    pub remote_username: Option<String>,
    pub remote_password: Option<String>,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub smartrest_templates: TemplatesSet,
    pub smartrest_one_templates: TemplatesSet,
    pub include_local_clean_session: AutoFlag,
    pub bridge_location: BridgeLocation,
    pub topic_prefix: TopicPrefix,
    pub profile_name: Option<ProfileName>,
    pub mqtt_schema: MqttSchema,
    pub keepalive_interval: Duration,
    pub proxy: Option<rumqttc::Proxy>,
}

impl From<BridgeConfigC8yParams> for BridgeConfig {
    fn from(params: BridgeConfigC8yParams) -> Self {
        let BridgeConfigC8yParams {
            mqtt_host,
            config_file,
            bridge_root_cert_path,
            remote_username,
            remote_password,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            smartrest_templates,
            smartrest_one_templates,
            include_local_clean_session,
            bridge_location,
            topic_prefix,
            profile_name,
            mqtt_schema,
            keepalive_interval,
            proxy,
        } = params;

        let mut topics: Vec<String> = vec![
            // Templates
            format!(r#"s/dt in 1 {topic_prefix}/ """#),
            format!(r#"s/ut/# out 1 {topic_prefix}/ """#),
            // Static templates
            format!(r#"s/us/# out 1 {topic_prefix}/ """#),
            format!(r#"t/us/# out 1 {topic_prefix}/ """#),
            format!(r#"q/us/# out 1 {topic_prefix}/ """#),
            format!(r#"c/us/# out 1 {topic_prefix}/ """#),
            format!(r#"s/ds in 1 {topic_prefix}/ """#),
            // Debug
            format!(r#"s/e in 0 {topic_prefix}/ """#),
            // SmartRest2
            format!(r#"s/uc/# out 1 {topic_prefix}/ """#),
            format!(r#"t/uc/# out 1 {topic_prefix}/ """#),
            format!(r#"q/uc/# out 1 {topic_prefix}/ """#),
            format!(r#"c/uc/# out 1 {topic_prefix}/ """#),
            format!(r#"s/dc/# in 1 {topic_prefix}/ """#),
            // c8y JSON
            format!(r#"inventory/managedObjects/update/# out 1 {topic_prefix}/ """#),
            format!(r#"measurement/measurements/create out 1 {topic_prefix}/ """#),
            format!(r#"measurement/measurements/createBulk out 1 {topic_prefix}/ """#),
            format!(r#"event/events/create out 1 {topic_prefix}/ """#),
            format!(r#"event/events/createBulk out 1 {topic_prefix}/ """#),
            format!(r#"alarm/alarms/create out 1 {topic_prefix}/ """#),
            format!(r#"alarm/alarms/createBulk out 1 {topic_prefix}/ """#),
            format!(r#"devicecontrol/notifications in 1 {topic_prefix}/ """#),
            format!(r#"error in 1 {topic_prefix}/ """#),
        ];

        let auth_type = if remote_username.is_some() {
            AuthType::Basic
        } else {
            AuthType::Certificate
        };

        if auth_type == AuthType::Certificate {
            topics.extend(vec![
                // c8y JWT token retrieval
                format!(r#"s/uat out 0 {topic_prefix}/ """#),
                format!(r#"s/dat in 0 {topic_prefix}/ """#),
            ]);
        }

        let templates_set = smartrest_templates
            .0
            .iter()
            .flat_map(|s| {
                // Smartrest templates should be deserialized as:
                // c8y/s/uc/template-1 (in from localhost), s/uc/template-1
                // c8y/s/dc/template-1 (out to localhost), s/dc/template-1
                [
                    format!(r#"s/uc/{s} out 1 {topic_prefix}/ """#),
                    format!(r#"s/dc/{s} in 1 {topic_prefix}/ """#),
                ]
                .into_iter()
            })
            .collect::<Vec<String>>();
        topics.extend(templates_set);

        // SmartRest1 (to support customers with existing solutions based on SmartRest 1)
        // Only add the topics if at least 1 template is defined
        if !smartrest_one_templates.0.is_empty() {
            topics.extend([
                format!(r#"s/ul/# out 1 {topic_prefix}/ """#),
                format!(r#"t/ul/# out 1 {topic_prefix}/ """#),
                format!(r#"q/ul/# out 1 {topic_prefix}/ """#),
                format!(r#"c/ul/# out 1 {topic_prefix}/ """#),
                format!(r#"s/dl/# in 1 {topic_prefix}/ """#),
            ]);

            let templates_set = smartrest_one_templates
                .0
                .iter()
                .flat_map(|s| {
                    // SmartRest1 templates should be deserialized as:
                    // c8y/s/ul/template-1 (in from localhost), s/ul/template-1
                    // c8y/s/dl/template-1 (out to localhost), s/dl/template-1
                    [
                        format!(r#"s/ul/{s} out 1 {topic_prefix}/ """#),
                        format!(r#"s/dl/{s} in 1 {topic_prefix}/ """#),
                    ]
                    .into_iter()
                })
                .collect::<Vec<String>>();
            topics.extend(templates_set);
        }

        let (include_local_clean_session, mosquitto_version) = match include_local_clean_session {
            AutoFlag::True => (true, None),
            AutoFlag::False => (false, None),
            AutoFlag::Auto => is_mosquitto_version_above_2(),
        };

        let service_name = format!("mosquitto-{topic_prefix}-bridge");
        let health = mqtt_schema.topic_for(
            &EntityTopicId::default_main_service(&service_name).unwrap(),
            &Channel::Health,
        );
        Self {
            cloud_name: "c8y".into(),
            config_file,
            connection: if let Some(profile) = &profile_name {
                format!("edge_to_c8y@{profile}")
            } else {
                "edge_to_c8y".into()
            },
            address: mqtt_host,
            remote_username,
            remote_password,
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: if let Some(profile) = &profile_name {
                format!("Cumulocity@{profile}")
            } else {
                "Cumulocity".into()
            },
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
            notification_topic: health.name,
            bridge_attempt_unsubscribe: false,
            topics,
            bridge_location,
            connection_check_attempts: 3,
            auth_type,
            mosquitto_version,
            keepalive_interval,
            proxy: proxy.map(ProxyWrapper),
        }
    }
}

#[derive(Debug)]
pub struct BridgeConfigC8yMqttServiceParams {
    pub mqtt_host: HostPort<MQTT_SVC_TLS_PORT>,
    pub config_file: Cow<'static, str>,
    pub remote_clientid: String,
    pub remote_username: Option<String>,
    pub remote_password: Option<String>,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub include_local_clean_session: AutoFlag,
    pub bridge_location: BridgeLocation,
    pub topic_prefix: TopicPrefix,
    pub profile_name: Option<ProfileName>,
    pub mqtt_schema: MqttSchema,
    pub keepalive_interval: Duration,
    pub sub_topics: TemplatesSet,
}

impl TryFrom<(&TEdgeConfig, Option<&ProfileName>)> for BridgeConfigC8yMqttServiceParams {
    type Error = ConfigError;

    fn try_from(value: (&TEdgeConfig, Option<&ProfileName>)) -> Result<Self, Self::Error> {
        let (config, profile) = value;

        let bridge_location = match config.mqtt.bridge.built_in {
            true => BridgeLocation::BuiltIn,
            false => BridgeLocation::Mosquitto,
        };
        let mqtt_schema = MqttSchema::with_root(config.mqtt.topic_root.clone());
        let c8y_config = config.c8y.try_get(profile)?;

        let (remote_username, remote_password) =
            match c8y_config.auth_method.to_type(&c8y_config.credentials_path) {
                AuthType::Certificate => (None, None),
                AuthType::Basic => {
                    let (username, password) = read_c8y_credentials(&c8y_config.credentials_path)?;
                    (Some(username), Some(password))
                }
            };

        let config_file = if let Some(profile_name) = &profile {
            format!("c8y-mqtt-svc@{profile_name}-bridge.conf")
        } else {
            "c8y-mqtt-svc-bridge.conf".to_string()
        };

        let params = BridgeConfigC8yMqttServiceParams {
            mqtt_host: c8y_config.mqtt_service.url.or_config_not_set()?.clone(),
            config_file: config_file.into(),
            bridge_root_cert_path: c8y_config.root_cert_path.clone().into(),
            remote_clientid: c8y_config.device.id()?.clone(),
            remote_username,
            remote_password,
            bridge_certfile: c8y_config.device.cert_path.clone().into(),
            bridge_keyfile: c8y_config.device.key_path.clone().into(),
            include_local_clean_session: c8y_config.bridge.include.local_cleansession.clone(),
            bridge_location,
            topic_prefix: c8y_config.mqtt_service.topic_prefix.clone(),
            profile_name: profile.cloned(),
            mqtt_schema,
            keepalive_interval: c8y_config.bridge.keepalive_interval.duration(),
            sub_topics: c8y_config.mqtt_service.topics.clone(),
        };

        Ok(params)
    }
}

impl From<BridgeConfigC8yMqttServiceParams> for BridgeConfig {
    fn from(params: BridgeConfigC8yMqttServiceParams) -> Self {
        let BridgeConfigC8yMqttServiceParams {
            mqtt_host,
            config_file,
            bridge_root_cert_path,
            remote_username,
            remote_password,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
            include_local_clean_session,
            bridge_location,
            topic_prefix,
            profile_name,
            mqtt_schema,
            keepalive_interval,
            sub_topics,
        } = params;

        let address = mqtt_host
            .to_string()
            .parse::<HostPort<MQTT_TLS_PORT>>()
            .expect("MQTT service address must be in the expected format");

        let mut topics: Vec<String> = vec![
            // Outgoing
            format!(r#"# out 1 {topic_prefix}/"#),
        ];

        // Topics to subscribe to
        for topic in sub_topics.0.iter() {
            topics.push(format!(r#"{topic} in 1 {topic_prefix}/"#));
        }

        let auth_type = if remote_password.is_some() {
            AuthType::Basic
        } else {
            AuthType::Certificate
        };

        let (include_local_clean_session, mosquitto_version) = match include_local_clean_session {
            AutoFlag::True => (true, None),
            AutoFlag::False => (false, None),
            AutoFlag::Auto => is_mosquitto_version_above_2(),
        };

        let service_name = format!("mosquitto-{topic_prefix}-bridge");
        let health = mqtt_schema.topic_for(
            &EntityTopicId::default_main_service(&service_name).unwrap(),
            &Channel::Health,
        );

        Self {
            cloud_name: "c8y-mqtt".into(),
            config_file,
            connection: if let Some(profile) = &profile_name {
                format!("edge_to_c8y_mqtt_service@{profile}")
            } else {
                "edge_to_c8y_mqtt_service".into()
            },
            address,
            remote_username,
            remote_password,
            bridge_root_cert_path,
            remote_clientid,
            local_clientid: if let Some(profile) = &profile_name {
                format!("CumulocityMqttService@{profile}")
            } else {
                "CumulocityMqttService".into()
            },
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
            notification_topic: health.name,
            bridge_attempt_unsubscribe: false,
            topics,
            bridge_location,
            connection_check_attempts: 3,
            auth_type,
            mosquitto_version,
            keepalive_interval,
            proxy: None,
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
pub fn is_mosquitto_version_above_2() -> (bool, Option<String>) {
    if let Ok(mosquitto) = which("mosquitto") {
        if let Ok(mosquitto_help) = Command::new(mosquitto).args(["--help"]).output() {
            if let Ok(help_content) = String::from_utf8(mosquitto_help.stdout) {
                let is_above_2 = help_content.starts_with("mosquitto version 2");
                return (
                    is_above_2,
                    Some(
                        help_content
                            .lines()
                            .next()
                            .unwrap_or("unknown")
                            .trim_start_matches("mosquitto version ")
                            .into(),
                    ),
                );
            }
        }
    }

    eprintln!("Failed to detect mosquitto version: assuming mosquitto version < 2.0.0");
    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    #[test]
    fn test_bridge_config_from_c8y_params() -> anyhow::Result<()> {
        use std::convert::TryFrom;
        let params = BridgeConfigC8yParams {
            mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
            config_file: "c8y-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            remote_username: None,
            remote_password: None,
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            smartrest_templates: TemplatesSet::from(vec!["abc", "def"]),
            smartrest_one_templates: TemplatesSet::default(),
            include_local_clean_session: AutoFlag::False,
            bridge_location: BridgeLocation::Mosquitto,
            topic_prefix: "c8y".try_into().unwrap(),
            profile_name: None,
            mqtt_schema: MqttSchema::with_root("te".into()),
            keepalive_interval: Duration::from_secs(60),
            proxy: None,
        };

        let bridge = BridgeConfig::from(params);

        let expected = BridgeConfig {
            cloud_name: "c8y".into(),
            config_file: "c8y-bridge.conf".into(),
            connection: "edge_to_c8y".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
            remote_username: None,
            remote_password: None,
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            remote_clientid: "alpha".into(),
            local_clientid: "Cumulocity".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: true,
            use_agent: true,
            topics: vec![
                // Templates
                r#"s/dt in 1 c8y/ """#.into(),
                r#"s/ut/# out 1 c8y/ """#.into(),
                // Static templates
                r#"s/us/# out 1 c8y/ """#.into(),
                r#"t/us/# out 1 c8y/ """#.into(),
                r#"q/us/# out 1 c8y/ """#.into(),
                r#"c/us/# out 1 c8y/ """#.into(),
                r#"s/ds in 1 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 1 c8y/ """#.into(),
                r#"t/uc/# out 1 c8y/ """#.into(),
                r#"q/uc/# out 1 c8y/ """#.into(),
                r#"c/uc/# out 1 c8y/ """#.into(),
                r#"s/dc/# in 1 c8y/ """#.into(),
                // c8y JSON
                r#"inventory/managedObjects/update/# out 1 c8y/ """#.into(),
                r#"measurement/measurements/create out 1 c8y/ """#.into(),
                r#"measurement/measurements/createBulk out 1 c8y/ """#.into(),
                r#"event/events/create out 1 c8y/ """#.into(),
                r#"event/events/createBulk out 1 c8y/ """#.into(),
                r#"alarm/alarms/create out 1 c8y/ """#.into(),
                r#"alarm/alarms/createBulk out 1 c8y/ """#.into(),
                r#"devicecontrol/notifications in 1 c8y/ """#.into(),
                r#"error in 1 c8y/ """#.into(),
                // c8y JWT token retrieval
                r#"s/uat out 0 c8y/ """#.into(),
                r#"s/dat in 0 c8y/ """#.into(),
                // Smartrest templates should be deserialized as:
                // s/uc/template-1 (in from localhost), s/uc/template-1
                // s/dc/template-1 (out to localhost), s/dc/template-1
                r#"s/uc/abc out 1 c8y/ """#.into(),
                r#"s/dc/abc in 1 c8y/ """#.into(),
                r#"s/uc/def out 1 c8y/ """#.into(),
                r#"s/dc/def in 1 c8y/ """#.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: false,
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "te/device/main/service/mosquitto-c8y-bridge/status/health".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 3,
            auth_type: AuthType::Certificate,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
            proxy: None,
        };

        assert_eq!(bridge, expected);

        Ok(())
    }

    #[test]
    fn test_bridge_config_from_c8y_params_basic_auth() -> anyhow::Result<()> {
        let params = BridgeConfigC8yParams {
            mqtt_host: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
            config_file: "c8y-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            remote_username: Some("octocat".into()),
            remote_password: Some("abcd1234".into()),
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            smartrest_templates: TemplatesSet::from(vec!["abc", "def"]),
            smartrest_one_templates: TemplatesSet::from(vec!["legacy1", "legacy2"]),
            include_local_clean_session: AutoFlag::False,
            bridge_location: BridgeLocation::Mosquitto,
            topic_prefix: "c8y".try_into().unwrap(),
            profile_name: Some("profile".parse().unwrap()),
            mqtt_schema: MqttSchema::with_root("te".into()),
            keepalive_interval: Duration::from_secs(60),
            proxy: None,
        };

        let bridge = BridgeConfig::from(params);

        let expected = BridgeConfig {
            cloud_name: "c8y".into(),
            config_file: "c8y-bridge.conf".into(),
            connection: "edge_to_c8y@profile".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io")?,
            remote_username: Some("octocat".into()),
            remote_password: Some("abcd1234".into()),
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            remote_clientid: "alpha".into(),
            local_clientid: "Cumulocity@profile".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: true,
            use_agent: true,
            topics: vec![
                // Templates
                r#"s/dt in 1 c8y/ """#.into(),
                r#"s/ut/# out 1 c8y/ """#.into(),
                // Static templates
                r#"s/us/# out 1 c8y/ """#.into(),
                r#"t/us/# out 1 c8y/ """#.into(),
                r#"q/us/# out 1 c8y/ """#.into(),
                r#"c/us/# out 1 c8y/ """#.into(),
                r#"s/ds in 1 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 1 c8y/ """#.into(),
                r#"t/uc/# out 1 c8y/ """#.into(),
                r#"q/uc/# out 1 c8y/ """#.into(),
                r#"c/uc/# out 1 c8y/ """#.into(),
                r#"s/dc/# in 1 c8y/ """#.into(),
                // c8y JSON
                r#"inventory/managedObjects/update/# out 1 c8y/ """#.into(),
                r#"measurement/measurements/create out 1 c8y/ """#.into(),
                r#"measurement/measurements/createBulk out 1 c8y/ """#.into(),
                r#"event/events/create out 1 c8y/ """#.into(),
                r#"event/events/createBulk out 1 c8y/ """#.into(),
                r#"alarm/alarms/create out 1 c8y/ """#.into(),
                r#"alarm/alarms/createBulk out 1 c8y/ """#.into(),
                r#"devicecontrol/notifications in 1 c8y/ """#.into(),
                r#"error in 1 c8y/ """#.into(),
                // Important: no c8y JWT token topics!
                // SmartRest2 custom templates
                r#"s/uc/abc out 1 c8y/ """#.into(),
                r#"s/dc/abc in 1 c8y/ """#.into(),
                r#"s/uc/def out 1 c8y/ """#.into(),
                r#"s/dc/def in 1 c8y/ """#.into(),
                // SmartREST 1.0 topics
                r#"s/ul/# out 1 c8y/ """#.into(),
                r#"t/ul/# out 1 c8y/ """#.into(),
                r#"q/ul/# out 1 c8y/ """#.into(),
                r#"c/ul/# out 1 c8y/ """#.into(),
                r#"s/dl/# in 1 c8y/ """#.into(),
                // SmartREST 1.0 custom templates
                r#"s/ul/legacy1 out 1 c8y/ """#.into(),
                r#"s/dl/legacy1 in 1 c8y/ """#.into(),
                r#"s/ul/legacy2 out 1 c8y/ """#.into(),
                r#"s/dl/legacy2 in 1 c8y/ """#.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: false,
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "te/device/main/service/mosquitto-c8y-bridge/status/health".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 3,
            auth_type: AuthType::Basic,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
            proxy: None,
        };

        assert_eq!(bridge, expected);

        Ok(())
    }

    #[test]
    fn test_bridge_config_from_c8y_mqtt_service_params_certificate_auth() -> anyhow::Result<()> {
        let params = BridgeConfigC8yMqttServiceParams {
            mqtt_host: HostPort::<MQTT_SVC_TLS_PORT>::try_from("test.test.io").unwrap(),
            config_file: "c8y-mqtt-svc-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            remote_username: None,
            remote_password: None,
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            include_local_clean_session: AutoFlag::False,
            bridge_location: BridgeLocation::Mosquitto,
            topic_prefix: "c8y-mqtt".try_into().unwrap(),
            profile_name: None,
            mqtt_schema: MqttSchema::with_root("te".into()),
            keepalive_interval: Duration::from_secs(45),
            sub_topics: TemplatesSet::from(vec!["test/topic", "demo/topic"]),
        };

        let bridge = BridgeConfig::from(params);

        let expected = BridgeConfig {
            cloud_name: "c8y-mqtt".into(),
            config_file: "c8y-mqtt-svc-bridge.conf".into(),
            connection: "edge_to_c8y_mqtt_service".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:9883")?,
            remote_username: None,
            remote_password: None,
            remote_clientid: "alpha".into(),
            local_clientid: "CumulocityMqttService".into(),
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: true,
            use_agent: true,
            topics: vec![
                "# out 1 c8y-mqtt/".into(),
                "test/topic in 1 c8y-mqtt/".into(),
                "demo/topic in 1 c8y-mqtt/".into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: false,
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "te/device/main/service/mosquitto-c8y-mqtt-bridge/status/health"
                .into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 3,
            auth_type: AuthType::Certificate,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(45),
            proxy: None,
        };

        assert_eq!(bridge, expected);
        Ok(())
    }

    #[test]
    fn test_bridge_config_from_c8y_mqtt_service_params_basic_auth() -> anyhow::Result<()> {
        let params = BridgeConfigC8yMqttServiceParams {
            mqtt_host: HostPort::<MQTT_SVC_TLS_PORT>::try_from("test.test.io")?,
            config_file: "c8y-mqtt-svc-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            remote_username: Some("octocat".into()),
            remote_password: Some("abcd1234".into()),
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            include_local_clean_session: AutoFlag::False,
            bridge_location: BridgeLocation::Mosquitto,
            topic_prefix: "c8y-mqtt".try_into().unwrap(),
            profile_name: None,
            mqtt_schema: MqttSchema::with_root("te".into()),
            keepalive_interval: Duration::from_secs(45),
            sub_topics: TemplatesSet::from(vec!["test/topic", "demo/topic"]),
        };

        let bridge = BridgeConfig::from(params);

        let expected = BridgeConfig {
            cloud_name: "c8y-mqtt".into(),
            config_file: "c8y-mqtt-svc-bridge.conf".into(),
            connection: "edge_to_c8y_mqtt_service".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:9883")?,
            remote_username: Some("octocat".into()),
            remote_password: Some("abcd1234".into()),
            remote_clientid: "alpha".into(),
            local_clientid: "CumulocityMqttService".into(),
            bridge_root_cert_path: Utf8PathBuf::from("./test_root.pem"),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: true,
            use_agent: true,
            topics: vec![
                "# out 1 c8y-mqtt/".into(),
                "test/topic in 1 c8y-mqtt/".into(),
                "demo/topic in 1 c8y-mqtt/".into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: false,
            local_clean_session: false,
            notifications: true,
            notifications_local_only: true,
            notification_topic: "te/device/main/service/mosquitto-c8y-mqtt-bridge/status/health"
                .into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 3,
            auth_type: AuthType::Basic,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(45),
            proxy: None,
        };

        assert_eq!(bridge, expected);
        Ok(())
    }
}
