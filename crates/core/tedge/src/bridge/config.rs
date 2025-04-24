use super::TEDGE_BRIDGE_CONF_DIR_PATH;
use crate::cli::CertificateShift;
use camino::Utf8PathBuf;
use core::fmt;
use std::borrow::Cow;
use std::time::Duration;
use tedge_config::models::auth_method::AuthType;
use tedge_config::models::HostPort;
use tedge_config::models::MQTT_TLS_PORT;
use tedge_config::TEdgeConfigLocation;
use tedge_utils::paths::DraftFile;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Eq, PartialEq)]
pub struct BridgeConfig {
    pub cloud_name: String,
    // XXX: having file name squished together with 20 fields which go into file content is a bit obscure
    pub config_file: Cow<'static, str>,
    pub connection: String,
    pub address: HostPort<MQTT_TLS_PORT>,
    pub remote_username: Option<String>,
    pub remote_password: Option<String>,
    pub bridge_root_cert_path: Utf8PathBuf,
    pub remote_clientid: String,
    pub local_clientid: String,
    pub bridge_certfile: Utf8PathBuf,
    pub bridge_keyfile: Utf8PathBuf,
    pub bridge_location: BridgeLocation,
    pub use_mapper: bool,
    pub use_agent: bool,
    pub try_private: bool,
    pub start_type: String,
    pub clean_session: bool,
    pub include_local_clean_session: bool,
    pub local_clean_session: bool,
    pub notifications: bool,
    pub notifications_local_only: bool,
    pub notification_topic: String,
    pub bridge_attempt_unsubscribe: bool,
    pub topics: Vec<String>,
    pub connection_check_attempts: u32,
    pub auth_type: AuthType,
    pub mosquitto_version: Option<String>,
    pub keepalive_interval: Duration,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum BridgeLocation {
    Mosquitto,
    BuiltIn,
}

impl fmt::Display for BridgeLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn => write!(f, "built-in"),
            Self::Mosquitto => write!(f, "mosquitto"),
        }
    }
}

macro_rules! writeln_async {
    ($writer:ident, $($fmt_args:expr),+ $(,)?) => {
        $writer.write_all(format!("{}\n", format_args!($($fmt_args),+)).as_bytes()).await
    };
}

impl BridgeConfig {
    pub async fn serialize<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln_async!(writer, "### Bridge")?;
        writeln_async!(writer, "connection {}", self.connection)?;
        writeln_async!(writer, "address {}", self.address)?;

        if std::fs::metadata(&self.bridge_root_cert_path)?.is_dir() {
            writeln_async!(writer, "bridge_capath {}", self.bridge_root_cert_path)?;
        } else {
            writeln_async!(writer, "bridge_cafile {}", self.bridge_root_cert_path)?;
        }

        writeln_async!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln_async!(writer, "local_clientid {}", self.local_clientid)?;

        if let Some(name) = &self.remote_username {
            writeln_async!(writer, "remote_username {}", name)?;
        }

        if self.auth_type == AuthType::Basic {
            if let Some(password) = &self.remote_password {
                writeln_async!(writer, "remote_password {}", password)?;
            }
        } else {
            writeln_async!(writer, "bridge_certfile {}", self.bridge_certfile)?;
            writeln_async!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        }

        writeln_async!(writer, "try_private {}", self.try_private)?;
        writeln_async!(writer, "start_type {}", self.start_type)?;
        writeln_async!(writer, "cleansession {}", self.clean_session)?;
        if self.include_local_clean_session {
            writeln_async!(writer, "local_cleansession {}", self.local_clean_session)?;
        }
        writeln_async!(writer, "notifications {}", self.notifications)?;
        writeln_async!(
            writer,
            "notifications_local_only {}",
            self.notifications_local_only
        )?;
        writeln_async!(writer, "notification_topic {}", self.notification_topic)?;
        writeln_async!(
            writer,
            "bridge_attempt_unsubscribe {}",
            self.bridge_attempt_unsubscribe
        )?;
        writeln_async!(
            writer,
            "keepalive_interval {}",
            self.keepalive_interval.as_secs()
        )?;

        writeln_async!(writer, "\n### Topics",)?;
        for topic in &self.topics {
            writeln_async!(writer, "topic {}", topic)?;
        }

        Ok(())
    }

    /// Returns a pair of certificates, the first being the primary active certificate
    /// and the second one being a candidate that has to be validated first
    /// before being promoted as the active certificate.
    ///
    /// Return `None` if the bridge doesn't use certificate or if there is no new certificate.
    pub async fn certificate_awaits_validation(&self) -> Option<CertificateShift> {
        if self.auth_type == AuthType::Basic {
            return None;
        }

        CertificateShift::exists_new_certificate(&self.bridge_certfile).await
    }

    /// Write the configuration file in a mosquitto configuration directory relative to the main
    /// tedge config location.
    pub async fn save(
        &self,
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<(), tedge_utils::paths::PathsError> {
        let dir_path = tedge_config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH);

        tedge_utils::paths::create_directories(dir_path)?;

        let config_path = self.file_path(tedge_config_location);
        let mut config_draft = DraftFile::new(config_path).await?.with_mode(0o644);
        self.serialize(&mut config_draft).await?;
        config_draft.persist().await?;

        Ok(())
    }

    fn file_path(&self, tedge_config_location: &TEdgeConfigLocation) -> Utf8PathBuf {
        tedge_config_location
            .tedge_config_root_path
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&*self.config_file)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use camino::Utf8Path;

    #[tokio::test]
    async fn test_serialize_with_cafile_correctly() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(file.path()).unwrap();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:8883")?,
            remote_username: None,
            remote_password: None,
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
            include_local_clean_session: true,
            local_clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 1,
            auth_type: AuthType::Certificate,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
        };

        let mut serialized_config = Vec::<u8>::new();
        bridge.serialize(&mut serialized_config).await?;

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
local_cleansession true
notifications false
notifications_local_only false
notification_topic test_topic
bridge_attempt_unsubscribe false
keepalive_interval 60

### Topics
"#,
        );

        assert_eq!(std::str::from_utf8(&serialized_config).unwrap(), expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_serialize_with_capath_correctly() -> anyhow::Result<()> {
        let dir = tempfile::TempDir::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(dir.path()).unwrap();

        let bridge = BridgeConfig {
            cloud_name: "test".into(),
            config_file: "test-bridge.conf".into(),
            connection: "edge_to_test".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:8883")?,
            remote_username: None,
            remote_password: None,
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
            include_local_clean_session: true,
            local_clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 1,
            auth_type: AuthType::Certificate,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
        };
        let mut serialized_config = Vec::<u8>::new();
        bridge.serialize(&mut serialized_config).await?;

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
local_cleansession true
notifications false
notifications_local_only false
notification_topic test_topic
bridge_attempt_unsubscribe false
keepalive_interval 60

### Topics
"#,
        );

        assert_eq!(std::str::from_utf8(&serialized_config).unwrap(), expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_serialize() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(file.path()).unwrap();

        let config = BridgeConfig {
            cloud_name: "az".into(),
            config_file: "az-bridge.conf".into(),
            connection: "edge_to_az".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:8883")?,
            remote_username: Some("test.test.io/alpha/?api-version=2018-06-30".into()),
            remote_password: None,
            bridge_root_cert_path: bridge_root_cert_path.to_owned(),
            remote_clientid: "alpha".into(),
            local_clientid: "Azure".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            use_agent: false,
            topics: vec![
                r#"messages/events/ out 1 az/ devices/alpha/"#.into(),
                r##"messages/devicebound/# in 1 az/ devices/alpha/"##.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: true,
            local_clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 1,
            auth_type: AuthType::Certificate,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
        };

        let mut buffer = Vec::new();
        config.serialize(&mut buffer).await?;

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
        expected.insert("local_cleansession true");
        expected.insert("notifications false");
        expected.insert("notifications_local_only false");
        expected.insert("notification_topic test_topic");
        expected.insert("bridge_attempt_unsubscribe false");
        expected.insert("keepalive_interval 60");

        expected.insert("topic messages/events/ out 1 az/ devices/alpha/");
        expected.insert("topic messages/devicebound/# in 1 az/ devices/alpha/");
        assert_eq!(config_set, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_serialize_use_basic_auth() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let bridge_root_cert_path = Utf8Path::from_path(file.path()).unwrap();

        let config = BridgeConfig {
            cloud_name: "c8y".into(),
            config_file: "c8y-bridge.conf".into(),
            connection: "edge_to_c8y".into(),
            address: HostPort::<MQTT_TLS_PORT>::try_from("test.test.io:8883")?,
            remote_username: Some("octocat".into()),
            remote_password: Some("pass1234".into()),
            bridge_root_cert_path: bridge_root_cert_path.to_owned(),
            remote_clientid: "alpha".into(),
            local_clientid: "C8Y".into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            use_mapper: false,
            use_agent: false,
            topics: vec![
                r#"inventory/managedObjects/update/# out 2 c8y/ """#.into(),
                r#"measurement/measurements/create out 2 c8y/ """#.into(),
            ],
            try_private: false,
            start_type: "automatic".into(),
            clean_session: true,
            include_local_clean_session: true,
            local_clean_session: true,
            notifications: false,
            notifications_local_only: false,
            notification_topic: "test_topic".into(),
            bridge_attempt_unsubscribe: false,
            bridge_location: BridgeLocation::Mosquitto,
            connection_check_attempts: 1,
            auth_type: AuthType::Basic,
            mosquitto_version: None,
            keepalive_interval: Duration::from_secs(60),
        };

        let mut buffer = Vec::new();
        config.serialize(&mut buffer).await?;

        let contents = String::from_utf8(buffer)?;
        let config_set: std::collections::HashSet<&str> = contents
            .lines()
            .filter(|str| !str.is_empty() && !str.starts_with('#'))
            .collect();

        let mut expected = std::collections::HashSet::new();
        expected.insert("connection edge_to_c8y");
        expected.insert("remote_username octocat");
        expected.insert("remote_password pass1234");
        expected.insert("address test.test.io:8883");
        let bridge_capath = format!("bridge_cafile {}", bridge_root_cert_path);
        expected.insert(&bridge_capath);
        expected.insert("remote_clientid alpha");
        expected.insert("local_clientid C8Y");
        expected.insert("start_type automatic");
        expected.insert("try_private false");
        expected.insert("cleansession true");
        expected.insert("local_cleansession true");
        expected.insert("notifications false");
        expected.insert("notifications_local_only false");
        expected.insert("notification_topic test_topic");
        expected.insert("bridge_attempt_unsubscribe false");
        expected.insert("keepalive_interval 60");
        expected.insert(r#"topic inventory/managedObjects/update/# out 2 c8y/ """#);
        expected.insert(r#"topic measurement/measurements/create out 2 c8y/ """#);
        assert_eq!(config_set, expected);
        Ok(())
    }
}
