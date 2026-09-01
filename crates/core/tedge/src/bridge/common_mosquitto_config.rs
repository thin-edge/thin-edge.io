use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;
use tokio::io::AsyncWriteExt;

const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListenerConfig {
    pub port: Option<u16>,
    pub bind_address: Option<String>,
    pub bind_interface: Option<String>,
    pub allow_anonymous: bool,
    pub capath: Option<Utf8PathBuf>,
    pub certfile: Option<Utf8PathBuf>,
    pub keyfile: Option<Utf8PathBuf>,
    pub require_certificate: bool,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            port: None,
            bind_address: None,
            bind_interface: None,
            allow_anonymous: false,
            capath: None,
            certfile: None,
            keyfile: None,
            require_certificate: true,
        }
    }
}

impl ListenerConfig {
    async fn maybe_writeln<W: AsyncWriteExt + Unpin + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: Option<D>,
    ) -> std::io::Result<()> {
        if let Some(value) = value {
            self.writeln(writer, key, value).await?;
        }
        Ok(())
    }

    async fn maybe_writeln_ca<W: AsyncWriteExt + Unpin + ?Sized>(
        &self,
        writer: &mut W,
        path: Option<&Utf8PathBuf>,
    ) -> std::io::Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        let key = if std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
            "capath"
        } else {
            "cafile"
        };
        self.writeln(writer, key, path).await
    }

    async fn writeln<W: AsyncWriteExt + Unpin + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: D,
    ) -> std::io::Result<()> {
        writer
            .write_all(format!("{key} {value}\n").as_bytes())
            .await
    }

    pub async fn write(&self, writer: &mut (impl AsyncWriteExt + Unpin)) -> std::io::Result<()> {
        let bind_address = self.bind_address.clone().unwrap_or_default();
        let maybe_listener = self
            .port
            .as_ref()
            .map(|port| format!("{} {}", port, bind_address));
        match maybe_listener {
            None => Ok(()),
            Some(listener) => {
                self.writeln(writer, "listener", listener).await?;
                self.writeln(writer, "allow_anonymous", self.allow_anonymous)
                    .await?;
                self.writeln(writer, "require_certificate", self.require_certificate)
                    .await?;
                if self.require_certificate {
                    self.writeln(writer, "use_identity_as_username", true)
                        .await?;
                }
                self.maybe_writeln(writer, "bind_interface", self.bind_interface.as_ref())
                    .await?;
                self.maybe_writeln_ca(writer, self.capath.as_ref()).await?;
                self.maybe_writeln(writer, "certfile", self.certfile.as_ref())
                    .await?;
                self.maybe_writeln(writer, "keyfile", self.keyfile.as_ref())
                    .await
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommonMosquittoConfig {
    pub config_file: String,
    pub internal_listener: ListenerConfig,
    pub external_listener: ListenerConfig,
    pub log_types: Vec<String>,
    pub message_size_limit: u32,
}

impl Default for CommonMosquittoConfig {
    fn default() -> Self {
        CommonMosquittoConfig {
            config_file: COMMON_MOSQUITTO_CONFIG_FILENAME.into(),
            internal_listener: ListenerConfig {
                port: Some(1883),
                bind_address: Some("127.0.0.1".into()),
                allow_anonymous: true,
                require_certificate: false,
                ..Default::default()
            },
            external_listener: Default::default(),
            log_types: vec![
                "error".into(),
                "warning".into(),
                "notice".into(),
                "information".into(),
                "subscribe".into(),
                "unsubscribe".into(),
            ],
            message_size_limit: 256 * 1024 * 1024 - 1,
        }
    }
}

impl CommonMosquittoConfig {
    pub async fn serialize<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b"per_listener_settings true\n").await?;

        writer.write_all(b"connection_messages true\n").await?;

        for log_type in &self.log_types {
            writer
                .write_all(format!("log_type {log_type}\n").as_bytes())
                .await?;
        }

        writer
            .write_all(format!("message_size_limit {}\n", self.message_size_limit).as_bytes())
            .await?;

        self.internal_listener.write(writer).await?;
        self.external_listener.write(writer).await?;
        writer.flush().await?;

        Ok(())
    }

    pub fn with_internal_opts(self, port: u16, bind_address: String) -> Self {
        let internal_listener = ListenerConfig {
            port: Some(port),
            bind_address: Some(bind_address),
            ..self.internal_listener
        };
        Self {
            internal_listener,
            ..self
        }
    }

    pub fn with_external_opts(
        self,
        port: Option<u16>,
        bind_address: Option<String>,
        bind_interface: Option<String>,
        capath: Option<Utf8PathBuf>,
        certfile: Option<Utf8PathBuf>,
        keyfile: Option<Utf8PathBuf>,
    ) -> Self {
        let mut external_listener = ListenerConfig {
            port,
            bind_address,
            bind_interface,
            capath,
            certfile,
            keyfile,
            allow_anonymous: true,
            require_certificate: false,
        };

        if external_listener.capath.is_some() {
            external_listener.allow_anonymous = false;
            external_listener.require_certificate = true;
        }

        Self {
            external_listener,
            ..self
        }
    }

    pub fn from_tedge_config(config: &TEdgeConfig) -> Self {
        CommonMosquittoConfig::default()
            .with_internal_opts(
                config.mqtt.bind.port.into(),
                config.mqtt.bind.address.to_string(),
            )
            .with_external_opts(
                config.mqtt.external.bind.port.or_none().cloned(),
                config
                    .mqtt
                    .external
                    .bind
                    .address
                    .or_none()
                    .cloned()
                    .map(|a| a.to_string()),
                config.mqtt.external.bind.interface.or_none().cloned(),
                config
                    .mqtt
                    .external
                    .ca_path
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
                config
                    .mqtt
                    .external
                    .cert_file
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
                config
                    .mqtt
                    .external
                    .key_file
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
            )
    }

    /// Write the configuration file in a mosquitto configuration directory relative to the main
    /// tedge config location.
    pub async fn save(
        &self,
        tedge_config: &TEdgeConfig,
    ) -> Result<(), tedge_utils::paths::PathsError> {
        let mut contents = Vec::new();
        self.serialize(&mut contents).await?;
        super::write_mosquitto_config(tedge_config, &self.config_file, &contents).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;

    #[tokio::test]
    async fn test_serialize() -> anyhow::Result<()> {
        let common_mosquitto_config = CommonMosquittoConfig::default();

        let mut buffer = Vec::new();
        common_mosquitto_config.serialize(&mut buffer).await?;

        let contents = String::from_utf8(buffer).unwrap();
        let config_set: std::collections::HashSet<&str> = contents
            .lines()
            .filter(|str| !str.is_empty() && !str.starts_with('#'))
            .collect();
        let mut expected = std::collections::HashSet::new();

        expected.insert("listener 1883 127.0.0.1");
        expected.insert("allow_anonymous true");
        expected.insert("connection_messages true");

        expected.insert("log_type error");
        expected.insert("log_type warning");
        expected.insert("log_type notice");
        expected.insert("log_type information");
        expected.insert("log_type subscribe");
        expected.insert("log_type unsubscribe");
        expected.insert("message_size_limit 268435455");
        expected.insert("per_listener_settings true");
        expected.insert("require_certificate false");

        assert_eq!(config_set, expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_serialize_with_opts() -> anyhow::Result<()> {
        let ca_dir = tempfile::TempDir::new()?;
        let ca_path = Utf8Path::from_path(ca_dir.path()).unwrap();
        let common_mosquitto_config = CommonMosquittoConfig::default();
        let mosquitto_config_with_opts = common_mosquitto_config
            .with_internal_opts(1234, "1.2.3.4".into())
            .with_external_opts(
                Some(2345),
                Some("0.0.0.0".to_string()),
                Some("wlan0".into()),
                Some(ca_path.to_owned()),
                Some("cert.pem".into()),
                Some("key.pem".into()),
            );

        assert!(mosquitto_config_with_opts
            .internal_listener
            .port
            .eq(&Some(1234)));

        let mut buffer = Vec::new();
        mosquitto_config_with_opts.serialize(&mut buffer).await?;

        let contents = String::from_utf8(buffer).unwrap();
        let expected = format!(
            "\
per_listener_settings true
connection_messages true
log_type error
log_type warning
log_type notice
log_type information
log_type subscribe
log_type unsubscribe
message_size_limit 268435455
listener 1234 1.2.3.4
allow_anonymous true
require_certificate false
listener 2345 0.0.0.0
allow_anonymous false
require_certificate true
use_identity_as_username true
bind_interface wlan0
capath {ca_path}
certfile cert.pem
keyfile key.pem
"
        );
        assert_eq!(contents, expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_serialize_external_cafile() -> anyhow::Result<()> {
        let ca_file = tempfile::NamedTempFile::new()?;
        let ca_path = Utf8Path::from_path(ca_file.path()).unwrap();
        let common_mosquitto_config = CommonMosquittoConfig::default().with_external_opts(
            Some(8883),
            Some("0.0.0.0".to_string()),
            None,
            Some(ca_path.to_owned()),
            Some("cert.pem".into()),
            Some("key.pem".into()),
        );

        let mut buffer = Vec::new();
        common_mosquitto_config.serialize(&mut buffer).await?;
        let contents = String::from_utf8(buffer).unwrap();

        assert!(contents.contains("use_identity_as_username true"));
        assert!(contents.contains(&format!("cafile {ca_path}")));
        assert!(!contents.contains(&format!("capath {ca_path}")));

        Ok(())
    }
}
