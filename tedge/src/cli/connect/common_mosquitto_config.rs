const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";

#[derive(Clone, Debug, PartialEq)]
pub struct CommonMosquittoConfig {
    pub config_file: String,
    pub listener: String,
    pub allow_anonymous: bool,
    pub connection_messages: bool,
    pub log_types: Vec<String>,
    pub message_size_limit: String,
}

impl Default for CommonMosquittoConfig {
    fn default() -> Self {
        CommonMosquittoConfig {
            config_file: COMMON_MOSQUITTO_CONFIG_FILENAME.into(),
            listener: "1883 localhost".into(),
            allow_anonymous: true,
            connection_messages: true,
            log_types: vec![
                "error".into(),
                "warning".into(),
                "notice".into(),
                "information".into(),
                "subscribe".into(),
                "unsubscribe".into(),
            ],
            message_size_limit: "256MB".into(),
        }
    }
}

impl CommonMosquittoConfig {
    pub fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "listener {}", self.listener)?;
        writeln!(writer, "allow_anonymous {}", self.allow_anonymous)?;
        writeln!(writer, "connection_messages {}", self.connection_messages)?;

        for log_type in &self.log_types {
            writeln!(writer, "log_type {}", log_type)?;
        }
        writeln!(writer, "message_size_limit {}", self.message_size_limit)?;

        Ok(())
    }

    pub fn with_port(self, port: u16) -> Self {
        let listener = port.to_string() + " localhost";
        Self { listener, ..self }
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
    expected.insert("message_size_limit 256MB");

    assert_eq!(config_set, expected);

    Ok(())
}

#[test]
fn test_serialize_with_port() -> anyhow::Result<()> {
    let common_mosquitto_config = CommonMosquittoConfig::default();
    let mosquitto_config_with_port = common_mosquitto_config.with_port(1234);

    assert!(mosquitto_config_with_port
        .listener
        .eq(&String::from("1234 localhost")));

    Ok(())
}
