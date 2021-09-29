const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";

#[derive(Clone, Debug, PartialEq)]
pub struct ListenerConfig {
    pub port: Option<u16>,
    pub bind_address: Option<String>,
    pub bind_interface: Option<String>,
    pub allow_anonymous: bool,
    pub capath: Option<String>,
    pub certfile: Option<String>,
    pub keyfile: Option<String>,
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
    fn maybe_writeln<W: std::io::Write + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: Option<D>,
    ) -> std::io::Result<()> {
        value
            .map(|v| self.writeln(writer, key, v))
            .unwrap_or(Ok(()))
    }
    fn writeln<W: std::io::Write + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: D,
    ) -> std::io::Result<()> {
        writeln!(writer, "{} {}", key, value)
    }
    pub fn write(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
        let bind_address = self.bind_address.clone().unwrap_or("".to_string());
        let maybe_listener = self
            .port
            .as_ref()
            .map(|port| format!("{} {}", port, bind_address));
        match maybe_listener {
            None => Ok(()),
            Some(listener) => {
                self.writeln(writer, "listener", listener)?;
                self.writeln(writer, "allow_anonymous", self.allow_anonymous)?;
                self.writeln(writer, "require_certificate", self.require_certificate)?;
                self.maybe_writeln(writer, "bind_interface", self.bind_interface.as_ref())?;
                self.maybe_writeln(writer, "capath", self.capath.as_ref())?;
                self.maybe_writeln(writer, "certfile", self.certfile.as_ref())?;
                self.maybe_writeln(writer, "keyfile", self.keyfile.as_ref())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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
                bind_address: Some("localhost".into()),
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
            message_size_limit: 268435455,
        }
    }
}

impl CommonMosquittoConfig {
    pub fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "per_listener_settings true")?;

        writeln!(writer, "connection_messages true")?;

        for log_type in &self.log_types {
            writeln!(writer, "log_type {}", log_type)?;
        }
        writeln!(
            writer,
            "message_size_limit {}",
            self.message_size_limit.to_string()
        )?;

        self.internal_listener.write(writer)?;
        self.external_listener.write(writer)?;

        Ok(())
    }

    pub fn with_internal_opts(self, port: u16) -> Self {
        let internal_listener = ListenerConfig {
            port: Some(port),
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
        capath: Option<String>,
        certfile: Option<String>,
        keyfile: Option<String>,
    ) -> Self {
        let external_listener = ListenerConfig {
            port,
            bind_address,
            bind_interface,
            capath: capath,
            certfile: certfile,
            keyfile: keyfile,
            ..self.external_listener
        };
        Self {
            external_listener,
            ..self
        }
    }
}

#[test]
fn test_serialize() -> anyhow::Result<()> {
    let common_mosquitto_config = CommonMosquittoConfig::default();

    let mut buffer = Vec::new();
    common_mosquitto_config.serialize(&mut buffer)?;

    let contents = String::from_utf8(buffer).unwrap();
    let config_set: std::collections::HashSet<&str> = contents
        .lines()
        .filter(|str| !str.is_empty() && !str.starts_with('#'))
        .collect();
    let mut expected = std::collections::HashSet::new();

    expected.insert("listener 1883 localhost");
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

#[test]
fn test_serialize_with_opts() -> anyhow::Result<()> {
    let common_mosquitto_config = CommonMosquittoConfig::default();
    let mosquitto_config_with_opts = common_mosquitto_config
        .with_internal_opts(1234)
        .with_external_opts(
            Some(2345),
            Some("0.0.0.0".into()),
            Some("wlan0".into()),
            Some("/etc/ssl/certs".into()),
            Some("cert.pem".into()),
            Some("key.pem".into()),
        );

    assert!(mosquitto_config_with_opts
        .internal_listener
        .port
        .eq(&Some(1234)));

    let mut buffer = Vec::new();
    mosquitto_config_with_opts.serialize(&mut buffer)?;

    let contents = String::from_utf8(buffer).unwrap();
    let expected = concat!(
        "per_listener_settings true\n",
        "connection_messages true\n",
        "log_type error\n",
        "log_type warning\n",
        "log_type notice\n",
        "log_type information\n",
        "log_type subscribe\n",
        "log_type unsubscribe\n",
        "message_size_limit 268435455\n",
        "listener 1234 localhost\n",
        "allow_anonymous true\n",
        "require_certificate false\n",
        "listener 2345 0.0.0.0\n",
        "allow_anonymous false\n",
        "require_certificate true\n",
        "bind_interface wlan0\n",
        "capath /etc/ssl/certs\n",
        "certfile cert.pem\n",
        "keyfile key.pem\n"
    );
    assert_eq!(contents, expected);

    Ok(())
}
