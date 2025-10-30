use crate::MqttMessage;
use crate::TopicFilter;
use certificate::parse_root_certificate;
use certificate::CertificateError;
use rumqttc::tokio_rustls::rustls;
use rumqttc::tokio_rustls::rustls::pki_types::CertificateDer;
use rumqttc::LastWill;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use zeroize::Zeroizing;

pub const MAX_PACKET_SIZE: usize = 268435455;

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
    /// Default: `16777216` (16 MB).
    pub max_packet_size: usize,

    /// LastWill message for a mqtt client
    ///
    /// Default: None
    pub last_will_message: Option<MqttMessage>,

    /// With first message on connection
    ///
    /// Default: None
    pub initial_message: Option<InitMessageFn>,
}

#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// MQTT host to connect to
    ///
    /// Default: "127.0.0.1"
    pub host: String,

    /// MQTT port to connect to. Usually it's either 1883 for insecure MQTT and
    /// 8883 for secure MQTT.
    ///
    /// Default: 1883
    pub port: u16,

    /// Authentication configuration
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
/// In addition, supporting username/password authentication with any combinations.
///
/// [mosquitto]: https://mosquitto.org/man/mosquitto-conf-5.html#authentication
#[derive(Debug, Clone)]
pub struct AuthenticationConfig {
    /// Trusted root certificate store used to verify broker certificate
    cert_store: rustls::RootCertStore,

    /// Client certificate and key
    cert_config: Option<ClientAuthCertConfig>,

    /// Client username
    username: Option<String>,

    /// Client password: it can be set only when username is set due to the MQTT specification.
    /// Therefore, the value can be read only via API.
    password: Option<Zeroizing<String>>,
}

impl Default for AuthenticationConfig {
    fn default() -> Self {
        AuthenticationConfig {
            cert_store: rustls::RootCertStore::empty(),
            cert_config: None,
            username: None,
            password: None,
        }
    }
}

impl AuthenticationConfig {
    pub fn get_cert_store_mut(&mut self) -> &mut rustls::RootCertStore {
        &mut self.cert_store
    }

    pub fn set_cert_config(
        &mut self,
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> Result<(), CertificateError> {
        let cert_config = ClientAuthCertConfig::new(cert_path.as_ref(), key_path.as_ref())?;
        self.cert_config = Some(cert_config);
        Ok(())
    }

    pub fn set_username(&mut self, username: String) {
        self.username = Some(username);
    }

    pub fn set_password(&mut self, password: Zeroizing<String>) {
        self.password = Some(password);
    }

    pub fn to_rustls_client_config(&self) -> Result<Option<rustls::ClientConfig>, rustls::Error> {
        if self.cert_store.is_empty() {
            return Ok(None);
        }

        let tls_config =
            rustls::ClientConfig::builder().with_root_certificates(self.cert_store.clone());

        let tls_config = match &self.cert_config {
            Some(cert_config) => tls_config.with_client_auth_cert(
                cert_config.cert_chain.clone(),
                cert_config.key.deref().0.clone_key(),
            )?,
            None => tls_config.with_no_client_auth(),
        };
        Ok(Some(tls_config))
    }

    /// When the password is empty, this returns an empty string.
    /// This is because `rumqttc::MqttOptions::set_credentials()` always requires a value for password.
    pub fn get_credentials(&self) -> Option<(String, Zeroizing<String>)> {
        match &self.username {
            Some(username) => {
                let password = self.password.clone().unwrap_or_default();
                Some((username.to_string(), password))
            }
            None => None,
        }
    }
}

#[derive(Clone)]
struct ClientAuthCertConfig {
    cert_chain: Vec<CertificateDer<'static>>,
    key: Arc<Zeroizing<PrivateKey>>,
}

impl ClientAuthCertConfig {
    pub fn new(
        cert_path: impl AsRef<Path>,
        key_path: impl AsRef<Path>,
    ) -> Result<Self, CertificateError> {
        let cert_chain = parse_root_certificate::read_cert_chain(cert_path)?;
        let key = parse_root_certificate::read_pvt_key(key_path)?;
        Ok(ClientAuthCertConfig {
            cert_chain,
            key: Arc::new(Zeroizing::new(PrivateKey(key))),
        })
    }
}

impl Debug for ClientAuthCertConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientAuthCertConfig")
            .field("cert_chain", &self)
            .finish()
    }
}

#[derive(Debug)]
struct PrivateKey(rustls::pki_types::PrivateKeyDer<'static>);

impl zeroize::Zeroize for PrivateKey {
    fn zeroize(&mut self) {
        self.0.zeroize()
    }
}

#[derive(Clone)]
pub struct InitMessageFn {
    initfn: Arc<Box<dyn Fn() -> MqttMessage + Send + Sync>>,
}

impl InitMessageFn {
    pub fn new(call_back: impl Fn() -> MqttMessage + Sync + Send + 'static) -> InitMessageFn {
        InitMessageFn {
            initfn: Arc::new(Box::new(call_back)),
        }
    }

    pub fn new_init_message(&self) -> MqttMessage {
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
                host: String::from("127.0.0.1"),
                port: 1883,
                authentication: None,
            },
            session_name: None,
            subscriptions: TopicFilter::empty(),
            clean_session: false,
            queue_capacity: 1024,
            max_packet_size: 16 * 1024 * 1024,
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
    pub fn with_last_will_message(self, lwm: MqttMessage) -> Self {
        Self {
            last_will_message: Some(lwm),
            ..self
        }
    }

    /// Clears the last will message
    pub fn with_no_last_will_or_initial_message(self) -> Self {
        Self {
            last_will_message: None,
            initial_message: None,
            ..self
        }
    }

    /// Set the initial message
    pub fn with_initial_message(
        self,
        initial_message: impl Fn() -> MqttMessage + Send + Sync + 'static,
    ) -> Self {
        Self {
            initial_message: Some(InitMessageFn::new(initial_message)),
            ..self
        }
    }

    pub fn with_client_auth(
        &mut self,
        config: AuthenticationConfig,
    ) -> Result<&mut Self, certificate::CertificateError> {
        self.broker.authentication.get_or_insert(config);
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
            if let Some((username, password)) = authentication_config.get_credentials() {
                mqtt_options.set_credentials(username, password.clone().to_string());
            }
            if let Some(tls_config) = authentication_config.to_rustls_client_config()? {
                mqtt_options.set_transport(rumqttc::Transport::tls_with_config(tls_config.into()));
            }
        }

        mqtt_options.set_max_packet_size(MAX_PACKET_SIZE, MAX_PACKET_SIZE);

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

/// Read the first line of the given file and return it.
pub fn read_password(path: impl AsRef<Path>) -> Result<Zeroizing<String>, CertificateError> {
    let f = File::open(&path).map_err(|error| CertificateError::IoError {
        error,
        path: path.as_ref().to_owned(),
    })?;
    let reader = BufReader::new(f);

    match reader.lines().next() {
        Some(Ok(password)) => Ok(Zeroizing::new(password)),
        Some(Err(error)) => Err(CertificateError::IoError {
            error,
            path: path.as_ref().to_owned(),
        }),
        None => Ok(Zeroizing::new("".to_string())),
    }
}
