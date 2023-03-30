use crate::Message;
use crate::TopicFilter;
use certificate::parse_root_certificate;
use rumqttc::tokio_rustls::rustls;
use rumqttc::LastWill;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::path::Path;
use std::sync::Arc;

/// Configuration of an MQTT connection
#[derive(Debug, Clone)]
pub struct Config {
    /// MQTT host to connect to
    ///
    /// Default: "localhost"
    pub host: String,

    /// MQTT port to connect to. Usually it's either 1883 for insecure MQTT and
    /// 8883 for secure MQTT.
    ///
    /// Default: 1883
    pub port: u16,

    /// The session name to be use on connect
    ///
    /// If no session name is provided, a random one will be created on connect,
    /// and the session will be clean on connect.
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
    /// Default: `1024 * 1024`.
    pub max_packet_size: usize,

    /// LastWill message for a mqtt client
    ///
    /// Default: None
    pub last_will_message: Option<Message>,

    /// With first message on connection
    ///
    /// Default: None
    pub initial_message: Option<InitMessageFn>,

    /// TLS configuration used to connect to the broker.
    pub cert_store: Option<rustls::RootCertStore>,
}

#[derive(Clone)]
pub struct InitMessageFn {
    initfn: Arc<Box<dyn Fn() -> Message + Send + Sync>>,
}

impl InitMessageFn {
    pub fn new(call_back: impl Fn() -> Message + Sync + Send + 'static) -> InitMessageFn {
        InitMessageFn {
            initfn: Arc::new(Box::new(call_back)),
        }
    }

    pub fn new_init_message(&self) -> Message {
        (*self.initfn)()
    }
}

impl Debug for InitMessageFn {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Init message creation function")
    }
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
            max_packet_size: 1024 * 1024,
            last_will_message: None,
            initial_message: None,
            cert_store: None,
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

    /// Set the last will message, this will be published when the mqtt connection gets closed.
    pub fn with_last_will_message(self, lwm: Message) -> Self {
        Self {
            last_will_message: Some(lwm),
            ..self
        }
    }

    /// Set the initial message
    pub fn with_initial_message(
        self,
        initial_message: impl Fn() -> Message + Send + Sync + 'static,
    ) -> Self {
        Self {
            initial_message: Some(InitMessageFn::new(initial_message)),
            ..self
        }
    }

    /// Adds all certificates present in `ca_file` file to the trust store.
    pub fn with_cafile(
        self,
        ca_file: impl AsRef<Path>,
    ) -> Result<Self, certificate::CertificateError> {
        let mut cert_store = self.cert_store.unwrap_or_else(rustls::RootCertStore::empty);
        parse_root_certificate::add_certs_from_file(&mut cert_store, ca_file)?;

        Ok(Self {
            cert_store: Some(cert_store),
            ..self
        })
    }

    /// Adds all certificate from all files in the directory `ca_dir` to the
    /// trust store.
    pub fn with_cadir(
        self,
        ca_dir: impl AsRef<Path>,
    ) -> Result<Self, certificate::CertificateError> {
        let mut cert_store = self.cert_store.unwrap_or_else(rustls::RootCertStore::empty);
        parse_root_certificate::add_certs_from_directory(&mut cert_store, ca_dir)?;

        Ok(Self {
            cert_store: Some(cert_store),
            ..self
        })
    }

    /// Wrap this config into an internal set of options for `rumqttc`.
    pub(crate) fn mqtt_options(&self) -> rumqttc::MqttOptions {
        let id = match &self.session_name {
            None => std::iter::repeat_with(fastrand::lowercase)
                .take(10)
                .collect(),
            Some(name) => name.clone(),
        };

        let mut mqtt_options = rumqttc::MqttOptions::new(id, &self.host, self.port);

        if self.session_name.is_none() {
            // There is no point to have a session with a random name that will not be reused.
            mqtt_options.set_clean_session(true);
        } else {
            mqtt_options.set_clean_session(self.clean_session);
        }

        if let Some(cert_store) = self.cert_store.as_ref() {
            let tls_config = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(cert_store.clone())
                .with_no_client_auth();

            mqtt_options.set_transport(rumqttc::Transport::tls_with_config(tls_config.into()));
        }

        mqtt_options.set_max_packet_size(self.max_packet_size, self.max_packet_size);

        if let Some(lwp) = &self.last_will_message {
            let last_will_message = LastWill {
                topic: lwp.topic.clone().into(),
                message: lwp.payload().clone().into(),
                qos: lwp.qos,
                retain: lwp.retain,
            };
            mqtt_options.set_last_will(last_will_message);
        }

        mqtt_options
    }
}
