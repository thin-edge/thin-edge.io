use crate::cli::connect::ConnectError;

use camino::Utf8PathBuf;
use reqwest::Url;

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfig {
    pub cloud_name: String,
    pub config_file: String,
    pub connection: String,
    pub address: String,
    pub remote_username: Option<String>,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub remote_clientid: String,
    pub local_clientid: String,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub use_mapper: bool,
    pub use_agent: bool,
    pub try_private: bool,
    pub start_type: String,
    pub clean_session: bool,
    pub notifications: bool,
    pub notifications_local_only: bool,
    pub notification_topic: String,
    pub bridge_attempt_unsubscribe: bool,
    pub topics: Vec<String>,
}

impl BridgeConfig {
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
            "notifications_local_only {}",
            self.notifications_local_only
        )?;
        writeln!(writer, "notification_topic {}", self.notification_topic)?;
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

    pub fn validate(&self) -> Result<(), ConnectError> {
        // XXX: This is actually wrong. Our address looks like this: `domain:port`
        // `Url::parse` will treat `domain` as `schema` ...
        Url::parse(&self.address)?;

        if !self.bridge_root_cert_path.exists() {
            return Err(ConnectError::Certificate);
        }

        if !self.bridge_certfile.exists() {
            return Err(ConnectError::Certificate);
        }

        if !self.bridge_keyfile.exists() {
            return Err(ConnectError::Certificate);
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use camino::Utf8Path;
    use camino::Utf8PathBuf;

    #[test]
    fn test_serialize_with_cafile_correctly() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(file.path()).unwrap();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: "test.test.io:8883".into(),
            remote_username: None,
            bridge_root_cert_path: bridge_root_cert_path.to_owned(),
            remote_clientid: "alpha".into(),
            local_clientid: "test".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            use_agent: false,
            topics: vec![],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
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
notifications_local_only false
notification_topic test_topic
bridge_attempt_unsubscribe false

### Topics
"#,
        );

        assert_eq!(serialized_config, expected.as_bytes());

        Ok(())
    }

    #[test]
    fn test_serialize_with_capath_correctly() -> anyhow::Result<()> {
        let dir = tempfile::TempDir::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(dir.path()).unwrap();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: "test.test.io:8883".into(),
            remote_username: None,
            bridge_root_cert_path: bridge_root_cert_path.to_owned(),
            remote_clientid: "alpha".into(),
            local_clientid: "test".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            use_agent: false,
            topics: vec![],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
        };
        let mut serialized_config = Vec::<u8>::new();
        bridge.serialize(&mut serialized_config)?;

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
notifications_local_only false
notification_topic test_topic
bridge_attempt_unsubscribe false

### Topics
"#,
        );

        assert_eq!(serialized_config, expected.as_bytes());

        Ok(())
    }

    #[test]
    fn test_serialize() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(file.path()).unwrap();

        let config = BridgeConfig {
            cloud_name: "az".into(),
            config_file: "az-bridge.conf".into(),
            connection: "edge_to_az".into(),
            address: "test.test.io:8883".into(),
            remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
            bridge_root_cert_path: bridge_root_cert_path.to_owned(),
            remote_clientid: "alpha".into(),
            local_clientid: "Azure".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            use_agent: false,
            topics: vec![
                r#"messages/events/ out 1 az/ devices/alpha/"#.into(),
                r##"messages/devicebound/# out 1 az/ devices/alpha/"##.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
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
        expected.insert("notifications_local_only false");
        expected.insert("notification_topic test_topic");
        expected.insert("bridge_attempt_unsubscribe false");

        expected.insert("topic messages/events/ out 1 az/ devices/alpha/");
        expected.insert("topic messages/devicebound/# out 1 az/ devices/alpha/");
        assert_eq!(config_set, expected);
        Ok(())
    }

    #[test]
    fn test_validate_ok() -> anyhow::Result<()> {
        let ca_file = tempfile::NamedTempFile::new()?;
        let bridge_ca_path = Utf8Path::from_path(ca_file.path()).unwrap();

        let cert_file = tempfile::NamedTempFile::new()?;
        let bridge_certfile = Utf8Path::from_path(cert_file.path()).unwrap().to_owned();

        let key_file = tempfile::NamedTempFile::new()?;
        let bridge_keyfile = Utf8Path::from_path(key_file.path()).unwrap().to_owned();

        let correct_url = "http://test.com";

        let config = BridgeConfig {
            address: correct_url.into(),
            bridge_root_cert_path: bridge_ca_path.to_owned(),
            bridge_certfile,
            bridge_keyfile,
            ..default_bridge_config()
        };

        assert!(config.validate().is_ok());

        Ok(())
    }

    // XXX: This test is flawed as it is not clear what it tests.
    // It can fail due to either `incorrect_url` OR `non_existent_path`.
    #[test]
    fn test_validate_wrong_url() {
        let incorrect_url = "noturl";
        let non_existent_path = Utf8PathBuf::from("/path/that/does/not/exist");

        let config = BridgeConfig {
            address: incorrect_url.into(),
            bridge_certfile: non_existent_path.clone(),
            bridge_keyfile: non_existent_path,
            ..default_bridge_config()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_wrong_cert_path() {
        let correct_url = "http://test.com";
        let non_existent_path = Utf8PathBuf::from("/path/that/does/not/exist");

        let config = BridgeConfig {
            address: correct_url.into(),
            bridge_certfile: non_existent_path.clone(),
            bridge_keyfile: non_existent_path,
            ..default_bridge_config()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_wrong_key_path() -> anyhow::Result<()> {
        let cert_file = tempfile::NamedTempFile::new()?;
        let bridge_certfile = Utf8Path::from_path(cert_file.path()).unwrap().to_owned();
        let correct_url = "http://test.com";
        let non_existent_path = "/path/that/does/not/exist";

        let config = BridgeConfig {
            address: correct_url.into(),
            bridge_certfile,
            bridge_keyfile: non_existent_path.into(),
            ..default_bridge_config()
        };

        assert!(config.validate().is_err());

        Ok(())
    }

    fn default_bridge_config() -> BridgeConfig {
        BridgeConfig {
            cloud_name: "az/c8y".into(),
            config_file: "cfg".to_string(),
            connection: "edge_to_az/c8y".into(),
            address: "".into(),
            remote_username: None,
            bridge_root_cert_path: "".into(),
            bridge_certfile: "".into(),
            bridge_keyfile: "".into(),
            remote_clientid: "".into(),
            local_clientid: "".into(),
            use_mapper: true,
            use_agent: true,
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
            topics: vec![],
        }
    }
}
