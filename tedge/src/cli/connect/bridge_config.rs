use crate::cli::connect::CommonMosquittoConfig;
use tedge_config::{ConnectUrl, FilePath};

#[derive(Debug, PartialEq)]
pub struct BridgeConfig {
    pub common_mosquitto_config: CommonMosquittoConfig,
    pub cloud_name: String,
    pub config_file: String,
    pub connection: String,
    pub address: String,
    pub remote_username: Option<String>,
    pub bridge_root_cert_path: FilePath,
    pub remote_clientid: String,
    pub local_clientid: String,
    pub bridge_certfile: FilePath,
    pub bridge_keyfile: FilePath,
    pub use_mapper: bool,
    pub try_private: bool,
    pub start_type: String,
    pub clean_session: bool,
    pub notifications: bool,
    pub bridge_attempt_unsubscribe: bool,
    pub topics: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub struct BridgeConfigParams {
    pub connect_url: ConnectUrl,
    pub mqtt_tls_port: u16,
    pub config_file: String,
    pub remote_clientid: String,
    pub bridge_root_cert_path: FilePath,
    pub bridge_certfile: FilePath,
    pub bridge_keyfile: FilePath,
}

impl BridgeConfig {
    pub fn new_for_c8y(params: BridgeConfigParams) -> Self {
        let BridgeConfigParams {
            connect_url,
            mqtt_tls_port,
            config_file,
            bridge_root_cert_path,
            remote_clientid,
            bridge_certfile,
            bridge_keyfile,
        } = params;

        let address = format!("{}:{}", connect_url.as_str(), mqtt_tls_port);

        Self {
            common_mosquitto_config: CommonMosquittoConfig::default(),
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
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                // Registration
                r#"s/dcr in 2 c8y/ """#.into(),
                r#"s/ucr out 2 c8y/ """#.into(),
                // Templates
                r#"s/dt in 2 c8y/ """#.into(),
                r#"s/ut/# out 2 c8y/ """#.into(),
                // Static templates
                r#"s/us out 2 c8y/ """#.into(),
                r#"t/us out 2 c8y/ """#.into(),
                r#"q/us out 2 c8y/ """#.into(),
                r#"c/us out 2 c8y/ """#.into(),
                r#"s/ds in 2 c8y/ """#.into(),
                r#"s/os in 2 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 2 c8y/ """#.into(),
                r#"t/uc/# out 2 c8y/ """#.into(),
                r#"q/uc/# out 2 c8y/ """#.into(),
                r#"c/uc/# out 2 c8y/ """#.into(),
                r#"s/dc/# in 2 c8y/ """#.into(),
                r#"s/oc/# in 2 c8y/ """#.into(),
                // c8y JSON
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
                r#"error in 2 c8y/ """#.into(),
            ],
        }
    }

    pub fn new_for_azure(params: BridgeConfigParams) -> Self {
        let BridgeConfigParams {
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
        let pub_msg_topic = format!("messages/events/ out 1 az/ devices/{}/", remote_clientid);
        let sub_msg_topic = format!(
            "messages/devicebound/# out 1 az/ devices/{}/",
            remote_clientid
        );
        Self {
            common_mosquitto_config: CommonMosquittoConfig::default(),
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
            use_mapper: false,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                pub_msg_topic,
                sub_msg_topic,
                r##"twin/res/# in 1 az/ $iothub/"##.into(),
                r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
            ],
        }
    }

    pub fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge")?;
        writeln!(writer, "connection {}", self.connection)?;
        match &self.remote_username {
            Some(name) => {
                writeln!(writer, "remote_username {}", name)?;
            }
            None => {}
        }
        writeln!(writer, "address {}", self.address)?;

        // XXX: This has to go away
        if std::fs::metadata(&self.bridge_root_cert_path)?.is_dir() {
            writeln!(writer, "bridge_capath {}", self.bridge_root_cert_path)?;
        } else {
            writeln!(writer, "bridge_cafile {}", self.bridge_root_cert_path)?;
        }

        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "local_clientid {}", self.local_clientid)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;
        writeln!(writer, "cleansession {}", self.clean_session)?;
        writeln!(writer, "notifications {}", self.notifications)?;
        writeln!(
            writer,
            "bridge_attempt_unsubscribe {}",
            self.bridge_attempt_unsubscribe
        )?;

        writeln!(writer, "\n### Topics",)?;
        for topic in &self.topics {
            writeln!(writer, "topic {}", topic)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::convert::TryFrom;
    use tempfile::NamedTempFile;

    #[test]
    fn test_new_for_c8y() -> anyhow::Result<()> {
        let params = BridgeConfigParams {
            connect_url: ConnectUrl::try_from("test.test.io")?,
            mqtt_tls_port: 8883,
            config_file: "c8y-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            bridge_root_cert_path: "./test_root.pem".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
        };

        let bridge = BridgeConfig::new_for_c8y(params);

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
            topics: vec![
                // Registration
                r#"s/dcr in 2 c8y/ """#.into(),
                r#"s/ucr out 2 c8y/ """#.into(),
                // Templates
                r#"s/dt in 2 c8y/ """#.into(),
                r#"s/ut/# out 2 c8y/ """#.into(),
                // Static templates
                r#"s/us out 2 c8y/ """#.into(),
                r#"t/us out 2 c8y/ """#.into(),
                r#"q/us out 2 c8y/ """#.into(),
                r#"c/us out 2 c8y/ """#.into(),
                r#"s/ds in 2 c8y/ """#.into(),
                r#"s/os in 2 c8y/ """#.into(),
                // Debug
                r#"s/e in 0 c8y/ """#.into(),
                // SmartRest2
                r#"s/uc/# out 2 c8y/ """#.into(),
                r#"t/uc/# out 2 c8y/ """#.into(),
                r#"q/uc/# out 2 c8y/ """#.into(),
                r#"c/uc/# out 2 c8y/ """#.into(),
                r#"s/dc/# in 2 c8y/ """#.into(),
                r#"s/oc/# in 2 c8y/ """#.into(),
                // c8y JSON
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
                r#"error in 2 c8y/ """#.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            common_mosquitto_config: CommonMosquittoConfig::default(),
        };

        assert_eq!(bridge, expected);

        Ok(())
    }

    #[test]
    fn test_new_for_azure() -> anyhow::Result<()> {
        let params = BridgeConfigParams {
            connect_url: ConnectUrl::try_from("test.test.io")?,
            mqtt_tls_port: 8883,
            config_file: "az-bridge.conf".into(),
            remote_clientid: "alpha".into(),
            bridge_root_cert_path: "./test_root.pem".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
        };

        let bridge = BridgeConfig::new_for_azure(params);

        let expected = BridgeConfig {
            cloud_name: "az".into(),
            config_file: "az-bridge.conf".to_string(),
            connection: "edge_to_az".into(),
            address: "test.test.io:8883".into(),
            remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
            bridge_root_cert_path: "./test_root.pem".into(),
            remote_clientid: "alpha".into(),
            local_clientid: "Azure".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            topics: vec![
                r#"messages/events/ out 1 az/ devices/alpha/"#.into(),
                r##"messages/devicebound/# out 1 az/ devices/alpha/"##.into(),
                r##"twin/res/# in 1 az/ $iothub/"##.into(),
                r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            common_mosquitto_config: CommonMosquittoConfig::default(),
        };

        assert_eq!(bridge, expected);

        Ok(())
    }

    #[test]
    fn test_serialize_with_cafile_correctly() -> anyhow::Result<()> {
        let file = NamedTempFile::new()?;
        let bridge_root_cert_path: FilePath = file.path().into();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: "test.test.io:8883".into(),
            remote_username: None,
            bridge_root_cert_path: bridge_root_cert_path.clone(),
            remote_clientid: "alpha".into(),
            local_clientid: "test".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            topics: vec![],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            common_mosquitto_config: CommonMosquittoConfig::default(),
        };

        let mut serialized_config = Vec::<u8>::new();
        bridge.serialize(&mut serialized_config)?;

        let bridge_cafile = format!("bridge_cafile {}", bridge_root_cert_path);
        let mut expected = r#"### Bridge
connection edge_to_test
address test.test.io:8883
"#
        .to_owned();

        expected.push_str(&bridge_cafile);
        expected.push_str(
            r#"
remote_clientid alpha
local_clientid test
bridge_certfile ./test-certificate.pem
bridge_keyfile ./test-private-key.pem
try_private false
start_type automatic
cleansession true
notifications false
bridge_attempt_unsubscribe false

### Topics
"#,
        );

        assert_eq!(serialized_config, expected.as_bytes());

        Ok(())
    }

    #[test]
    fn test_serialize_with_capath_correctly() {
        let dir = tempfile::TempDir::new().unwrap();
        let bridge_root_cert_path: FilePath = dir.path().into();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: "test.test.io:8883".into(),
            remote_username: None,
            bridge_root_cert_path: bridge_root_cert_path.clone(),
            remote_clientid: "alpha".into(),
            local_clientid: "test".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            topics: vec![],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            common_mosquitto_config: CommonMosquittoConfig::default(),
        };
        let mut serialized_config = Vec::<u8>::new();
        bridge.serialize(&mut serialized_config).unwrap();

        let bridge_capath = format!("bridge_capath {}", bridge_root_cert_path);
        let mut expected = r#"### Bridge
connection edge_to_test
address test.test.io:8883
"#
        .to_owned();

        expected.push_str(&bridge_capath);
        expected.push_str(
            r#"
remote_clientid alpha
local_clientid test
bridge_certfile ./test-certificate.pem
bridge_keyfile ./test-private-key.pem
try_private false
start_type automatic
cleansession true
notifications false
bridge_attempt_unsubscribe false

### Topics
"#,
        );

        assert_eq!(serialized_config, expected.as_bytes());
    }

    #[test]
    fn test_serialize() -> anyhow::Result<()> {
        let file = NamedTempFile::new()?;
        let bridge_root_cert_path: FilePath = file.path().into();

        let config = BridgeConfig {
            cloud_name: "az".into(),
            config_file: "az-bridge.conf".into(),
            connection: "edge_to_az".into(),
            address: "test.test.io:8883".into(),
            remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
            bridge_root_cert_path: bridge_root_cert_path.clone(),
            remote_clientid: "alpha".into(),
            local_clientid: "Azure".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            topics: vec![
                r#"messages/events/ out 1 az/ devices/alpha/"#.into(),
                r##"messages/devicebound/# out 1 az/ devices/alpha/"##.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            common_mosquitto_config: CommonMosquittoConfig::default(),
        };

        let mut buffer = Vec::new();
        config.serialize(&mut buffer)?;

        let contents = String::from_utf8(buffer)?;
        let config_set: std::collections::HashSet<&str> = contents
            .lines()
            .filter(|str| !str.is_empty() && !str.starts_with('#'))
            .collect();

        let mut expected = std::collections::HashSet::new();
        expected.insert("connection edge_to_az");
        expected.insert("remote_username test.test.io/alpha/?api-version=2018-06-30");
        expected.insert("address test.test.io:8883");
        let bridge_capath = format!("bridge_cafile {}", bridge_root_cert_path);
        expected.insert(&bridge_capath);
        expected.insert("remote_clientid alpha");
        expected.insert("local_clientid Azure");
        expected.insert("bridge_certfile ./test-certificate.pem");
        expected.insert("bridge_keyfile ./test-private-key.pem");
        expected.insert("start_type automatic");
        expected.insert("try_private false");
        expected.insert("cleansession true");
        expected.insert("notifications false");
        expected.insert("bridge_attempt_unsubscribe false");

        expected.insert("topic messages/events/ out 1 az/ devices/alpha/");
        expected.insert("topic messages/devicebound/# out 1 az/ devices/alpha/");

        assert_eq!(config_set, expected);

        Ok(())
    }
}
