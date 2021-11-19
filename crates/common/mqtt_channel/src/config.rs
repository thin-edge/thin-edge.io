/// Configuration of an MQTT connection
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,

    /// Clean the MQTT session upon connect if set to `true`.
    ///
    /// Default: `false`.
    pub clean_session: bool,

    /// Capacity of the internal message queues
    ///
    /// Default: `1024`.
    pub queue_capacity: usize,
}

/// By default a client connects the local MQTT broker.
impl Default for Config {
    fn default() -> Self {
        Config {
            host: String::from("localhost"),
            port: 1883,
            clean_session: false,
            queue_capacity: 1024,
        }
    }
}

impl Config {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Config::default()
        }
    }

    /// Set a custom port
    pub fn with_port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    /// Set the clean_session flag
    pub fn clean_session(self) -> Self {
        Self {
            clean_session: true,
            ..self
        }
    }

    /// Set the queue capacity
    pub fn with_queue_capacity(self, queue_capacity: usize) -> Self {
        Self {
            queue_capacity,
            ..self
        }
    }
}
