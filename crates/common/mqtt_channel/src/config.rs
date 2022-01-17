use crate::TopicFilter;

/// Configuration of an MQTT connection
#[derive(Debug, Clone)]
pub struct Config {
    /// MQTT host to connect to
    ///
    /// Default: "localhost"
    pub host: String,

    /// MQTT port to connect to
    ///
    /// Default: 1883
    pub port: u16,

    /// The session name to be use on connect
    ///
    /// If no session name is provided, a random one will be created on connect.
    ///
    /// Default: None
    pub session_name: Option<String>,

    /// The list of topics to subscribe to on connect
    ///
    /// Default: An empty topic list
    pub subscriptions: TopicFilter,

    /// Clean the MQTT session upon connect if set to `true`.
    ///
    /// Default: `false`.
    pub clean_session: bool,

    /// Capacity of the internal message queues
    ///
    /// Default: `1024`.
    ///
    pub queue_capacity: usize,

    /// Maximum size for a message payload
    ///
    /// Default: `8 * 1024`.
    pub max_packet_size: usize,
}

/// By default a client connects the local MQTT broker.
impl Default for Config {
    fn default() -> Self {
        Config {
            host: String::from("localhost"),
            port: 1883,
            session_name: None,
            subscriptions: TopicFilter::empty(),
            clean_session: false,
            queue_capacity: 1024,
            max_packet_size: 8 * 1024,
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

    /// Set a custom host
    pub fn with_host(self, host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            ..self
        }
    }

    /// Set a custom port
    pub fn with_port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    /// Set the session name
    pub fn with_session_name(self, name: impl Into<String>) -> Self {
        Self {
            session_name: Some(name.into()),
            ..self
        }
    }

    /// Add a list of topics to subscribe to on connect
    ///
    /// Can be called several times to subscribe to many topics.
    pub fn with_subscriptions(mut self, topics: TopicFilter) -> Self {
        self.subscriptions.add_all(topics);
        self
    }

    /// Set the clean_session flag
    pub fn with_clean_session(self, flag: bool) -> Self {
        Self {
            clean_session: flag,
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

    /// Set the maximum size for a message payload
    pub fn with_max_packet_size(self, max_packet_size: usize) -> Self {
        Self {
            max_packet_size,
            ..self
        }
    }
}
