use crate::Message;
use crate::TopicFilter;
use certificate::parse_root_certificate;
use certificate::CertificateError;
use rumqttc::tokio_rustls::rustls;
use rumqttc::tokio_rustls::rustls::Certificate;
use rumqttc::tokio_rustls::rustls::PrivateKey;
use rumqttc::LastWill;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::path::Path;
use std::sync::Arc;

/// Configuration of an MQTT connection
#[derive(Debug, Clone)]
pub struct Config {
    /// The struct containing all the necessary properties to connect to a broker.
    pub broker: BrokerConfig,

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
}

#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// MQTT host to connect to
    ///
    /// Default: "localhost"
    pub host: String,

    /// MQTT port to connect to. Usually it's either 1883 for insecure MQTT and
    /// 8883 for secure MQTT.
    ///
    /// Default: 1883
    pub port: u16,

    /// Certificate authentication configuration
    pub authentication: Option<AuthenticationConfig>,
}

/// MQTT certificate authentication configuration.
///
/// Intended to mirror authentication model found in the [mosquitto] MQTT
/// broker. In short, there are 3 supported modes of connecting:
///
/// 1. no authentication
/// 2. server authentication - clients will verify MQTT broker certificate
/// 3. server and client authentication - clients will verify MQTT broker
///    certificate and broker will verify client certificates
///
/// [mosquitto]: https://mosquitto.org/man/mosquitto-conf-5.html#authentication
#[derive(Debug, Clone)]
pub struct AuthenticationConfig {
    /// Trusted root certificate store used to verify broker certificate
    cert_store: rustls::RootCertStore,

    /// Client authentication configuration
    client_auth: Option<ClientAuthConfig>,
}

impl Default for AuthenticationConfig {
    fn default() -> Self {
        AuthenticationConfig {
            cert_store: rustls::RootCertStore::empty(),
            client_auth: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ClientAuthConfig {
    cert_chain: Vec<Certificate>,
    key: PrivateKey,
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
            broker: BrokerConfig {
                host: String::from("localhost"),
                port: 1883,
                authentication: None,
            },
            session_name: None,
            subscriptions: TopicFilter::empty(),
            clean_session: false,
            queue_capacity: 1024,
            max_packet_size: 1024 * 1024,
            last_will_message: None,
            initial_message: None,
        }
    }
}

impl Config {
    pub fn with_broker(broker_config: BrokerConfig) -> Self {
        Self {
            broker: broker_config,
            ..Config::default()
        }
    }

    /// Set a custom host
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.broker.host = host.into();
        self
    }

    /// Set a custom port
    pub fn with_port(mut self, port: u16) -> Self {
        self.broker.port = port;
        self
    }

    /// Set the session name
    pub fn with_session_name(self, name: impl Into<String>) -> Self {
        Self {
            session_name: Some(name.into()),
            ..self
        }
    }

    /// Unset the session name and clear the session
    pub fn with_no_session(self) -> Self {
        Self {
            session_name: None,
            clean_session: true,
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
    /// Enables server authentication.
    pub fn with_cafile(
        &mut self,
        ca_file: impl AsRef<Path>,
    ) -> Result<&mut Self, certificate::CertificateError> {
        let authentication_config = self.broker.authentication.get_or_insert(Default::default());
        let cert_store = &mut authentication_config.cert_store;

        parse_root_certificate::add_certs_from_file(cert_store, ca_file)?;

        Ok(self)
    }

    /// Adds all certificate from all files in the directory `ca_dir` to the
    /// trust store. Enables server authentication.
    pub fn with_cadir(
        &mut self,
        ca_dir: impl AsRef<Path>,
    ) -> Result<&mut Self, certificate::CertificateError> {
        let authentication_config = self.broker.authentication.get_or_insert(Default::default());
        let cert_store = &mut authentication_config.cert_store;

        parse_root_certificate::add_certs_from_directory(cert_store, ca_dir)?;

        Ok(self)
    }

    /// Provide client certificate and private key for authentication. If server
    /// authentication was not enabled by previously calling
    /// [`Config::with_cafile`] or [`Config::with_cadir`], this method also
    /// enables it but initializes an empty root cert store.
    pub fn with_client_auth<P: AsRef<Path>>(
        &mut self,
        cert_file: P,
        key_file: P,
    ) -> Result<&mut Self, CertificateError> {
        let cert_chain = parse_root_certificate::read_cert_chain(cert_file)?;
        let key = parse_root_certificate::read_pvt_key(key_file)?;

        let client_auth_config = ClientAuthConfig { cert_chain, key };

        let authentication_config = self.broker.authentication.get_or_insert(Default::default());
        authentication_config.client_auth = Some(client_auth_config);

        Ok(self)
    }

    /// Wrap this config into an internal set of options for `rumqttc`.
    pub fn rumqttc_options(&self) -> Result<rumqttc::MqttOptions, rustls::Error> {
        let id = match &self.session_name {
            None => std::iter::repeat_with(fastrand::lowercase)
                .take(10)
                .collect(),
            Some(name) => name.clone(),
        };

        let broker_config = &self.broker;

        let mut mqtt_options =
            rumqttc::MqttOptions::new(id, &broker_config.host, broker_config.port);

        if self.session_name.is_none() {
            // There is no point to have a session with a random name that will not be reused.
            mqtt_options.set_clean_session(true);
        } else {
            mqtt_options.set_clean_session(self.clean_session);
        }

        if let Some(authentication_config) = &broker_config.authentication {
            let tls_config = rustls::ClientConfig::builder()
                .with_safe_defaults()
                .with_root_certificates(authentication_config.cert_store.clone());

            let tls_config = match authentication_config.client_auth.clone() {
                Some(client_auth_config) => tls_config
                    .with_single_cert(client_auth_config.cert_chain, client_auth_config.key)?,
                None => tls_config.with_no_client_auth(),
            };

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

        Ok(mqtt_options)
    }
}
