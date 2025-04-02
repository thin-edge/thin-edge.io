use crate::proxy_url::Protocol;
use crate::proxy_url::ProxyUrlGenerator;
use crate::smartrest::error::SmartRestDeserializerError;
use crate::smartrest::smartrest_deserializer::SmartRestJwtResponse;
use base64::prelude::*;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use mqtt_channel::Connection;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use reqwest::header::HeaderValue;
use reqwest::header::InvalidHeaderValue;
use reqwest::Url;
use std::path::PathBuf;
use std::time::Duration;
use tedge_config::models::auth_method::AuthType;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::ConfigNotSet;
use tedge_config::tedge_toml::MultiError;
use tedge_config::tedge_toml::ReadError;
use tedge_config::CertificateError;
use tedge_config::TEdgeConfig;
use tracing::debug;
use tracing::error;
use tracing::info;

#[derive(thiserror::Error, Debug)]
pub enum C8yEndPointError {
    #[error("Cumulocity internal id not found for the device: {0}")]
    InternalIdNotFound(String),
}

/// Define a C8y endpoint
#[derive(Debug, Clone)]
pub struct C8yEndPoint {
    c8y_host: String,
    c8y_mqtt_host: String,
    proxy: ProxyUrlGenerator,
}

impl C8yEndPoint {
    pub fn from_config(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yEndPointConfigError> {
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let c8y_host = c8y_config.http.or_config_not_set()?.to_string();
        let c8y_mqtt_host = c8y_config.mqtt.or_config_not_set()?.to_string();
        let auth_proxy_addr = c8y_config.proxy.client.host.clone();
        let auth_proxy_port = c8y_config.proxy.client.port;
        let auth_proxy_protocol = c8y_config
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);
        let proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, auth_proxy_protocol);

        Ok(C8yEndPoint {
            c8y_host,
            c8y_mqtt_host,
            proxy,
        })
    }

    pub fn local_proxy(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yEndPointConfigError> {
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let auth_proxy_addr = c8y_config.proxy.client.host.clone();
        let auth_proxy_port = c8y_config.proxy.client.port;
        let auth_proxy_protocol = c8y_config
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);
        let c8y_host = format!(
            "{}://{auth_proxy_addr}:{auth_proxy_port}",
            auth_proxy_protocol.as_str()
        );
        let c8y_mqtt_host = c8y_host.clone();
        let proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, auth_proxy_protocol);
        Ok(C8yEndPoint {
            c8y_host,
            c8y_mqtt_host,
            proxy,
        })
    }

    pub fn new(c8y_host: &str, c8y_mqtt_host: &str, proxy: ProxyUrlGenerator) -> C8yEndPoint {
        C8yEndPoint {
            c8y_host: c8y_host.into(),
            c8y_mqtt_host: c8y_mqtt_host.into(),
            proxy,
        }
    }

    pub fn get_base_url(&self) -> String {
        let c8y_host = &self.c8y_host;
        if c8y_host.starts_with("http") {
            c8y_host.to_string()
        } else {
            format!("https://{c8y_host}")
        }
    }

    pub fn get_proxy_url(&self) -> String {
        self.proxy.base_url()
    }

    pub fn proxy_url_for_internal_id(&self, device_id: &str) -> String {
        Self::url_for_internal_id(&self.proxy.base_url(), device_id)
    }

    pub fn proxy_url_for_sw_list(&self, internal_id: &str) -> String {
        Self::url_for_sw_list(&self.proxy.base_url(), internal_id)
    }

    pub fn proxy_url_for_create_event(&self) -> String {
        Self::url_for_create_event(&self.proxy.base_url())
    }

    pub fn proxy_url_for_event_binary_upload(&self, event_id: &str) -> Url {
        let url = Self::url_for_event_binary_upload(&self.proxy.base_url(), event_id);
        Url::parse(&url).unwrap()
    }

    fn url_for_sw_list(host: &str, internal_id: &str) -> String {
        format!("{host}/inventory/managedObjects/{internal_id}")
    }

    fn url_for_internal_id(host: &str, device_id: &str) -> String {
        format!("{host}/identity/externalIds/c8y_Serial/{device_id}")
    }

    fn url_for_create_event(host: &str) -> String {
        format!("{host}/event/events/")
    }

    fn url_for_event_binary_upload(host: &str, event_id: &str) -> String {
        format!("{host}/event/events/{event_id}/binaries")
    }

    pub fn c8y_url_for_internal_id(&self, device_id: &str) -> String {
        Self::url_for_internal_id(&self.get_base_url(), device_id)
    }

    pub fn c8y_url_for_sw_list(&self, internal_id: &str) -> String {
        Self::url_for_sw_list(&self.get_base_url(), internal_id)
    }

    pub fn c8y_url_for_create_event(&self) -> String {
        Self::url_for_create_event(&self.get_base_url())
    }

    pub fn c8y_url_for_event_binary_upload(&self, event_id: &str) -> Url {
        let url = Self::url_for_event_binary_upload(&self.get_base_url(), event_id);
        Url::parse(&url).unwrap()
    }

    // Return the local url going through the local auth proxy to reach the given remote url
    //
    // Return the remote url unchanged if not related to the current tenant.
    pub fn local_proxy_url(&self, remote_url: &str) -> Result<Url, InvalidUrl> {
        let valid_url: Url = remote_url.parse().map_err(|err| InvalidUrl {
            url: remote_url.to_string(),
            err,
        })?;
        Ok(self
            .maybe_tenant_url(valid_url.as_str())
            .filter(|tenant_url| tenant_url.scheme().starts_with("http"))
            .map(|tenant_url| self.proxy.proxy_url(tenant_url))
            .unwrap_or(valid_url))
    }

    fn maybe_tenant_url(&self, url: &str) -> Option<Url> {
        // c8y URL may contain either `Tenant Name` or Tenant Id` so they can be one of following options:
        // * <tenant_name>.<domain> eg: sample.c8y.io
        // * <tenant_id>.<domain> eg: t12345.c8y.io
        // These URLs may be both equivalent and point to the same tenant.
        // We are going to remove that and only check if the domain is the same.
        let (tenant_http_host, _port) = self
            .c8y_host
            .split_once(':')
            .unwrap_or((&self.c8y_host, ""));
        let (tenant_mqtt_host, _port) = self
            .c8y_mqtt_host
            .split_once(':')
            .unwrap_or((&self.c8y_mqtt_host, ""));
        let url = Url::parse(url).ok()?;
        let url_host = url.domain()?;

        let (_, host) = url_host.split_once('.').unwrap_or(("", url_host));
        let (_, c8y_http_host) = tenant_http_host
            .split_once('.')
            .unwrap_or(("", tenant_http_host));
        let (_, c8y_mqtt_host) = tenant_mqtt_host
            .split_once('.')
            .unwrap_or(("", tenant_mqtt_host));

        // The configured `c8y.http` setting may have a port value specified,
        // but the incoming URL is less likely to have any port specified.
        // Hence just matching the host prefix.
        (host == c8y_http_host || host == c8y_mqtt_host).then_some(url)
    }
}

/// The errors that could occur while building `C8yEndPoint` struct.
#[derive(Debug, thiserror::Error)]
pub enum C8yEndPointConfigError {
    #[error(transparent)]
    FromReadError(#[from] ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    FromMultiError(#[from] MultiError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid url {url}: {err}")]
pub struct InvalidUrl {
    url: String,
    err: url::ParseError,
}

pub enum C8yAuthRetriever {
    Basic {
        credentials_path: Utf8PathBuf,
    },
    Jwt {
        mqtt_config: Box<mqtt_channel::Config>,
        topic_prefix: TopicPrefix,
    },
}

/// The credential file representation. e.g.:
/// ```toml
/// [c8y]
/// username = "t1234/octocat"
/// password = "abcd1234"
/// ```
#[derive(Debug, serde::Deserialize)]
struct Credentials {
    c8y: BasicCredentials,
}

#[derive(Debug, serde::Deserialize)]
struct BasicCredentials {
    username: String,
    password: String,
}

#[derive(thiserror::Error, Debug)]
pub enum C8yAuthRetrieverError {
    #[error(transparent)]
    ConfigMulti(#[from] MultiError),

    #[error(transparent)]
    JwtError(#[from] JwtError),

    #[error(transparent)]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error(transparent)]
    CredentialsFileError(#[from] CredentialsFileError),

    #[error("Failed to load certificates for MQTT connection")]
    Certificate(#[from] CertificateError),
}

impl C8yAuthRetriever {
    pub fn from_tedge_config(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yAuthRetrieverError> {
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let topic_prefix = c8y_config.bridge.topic_prefix.clone();

        match c8y_config.auth_method.to_type(&c8y_config.credentials_path) {
            AuthType::Basic => Ok(Self::Basic {
                credentials_path: c8y_config.credentials_path.clone(),
            }),
            AuthType::Certificate => {
                let mqtt_config = tedge_config.mqtt_config()?;

                let topic = TopicFilter::new_unchecked(&format!("{topic_prefix}/s/dat"));
                let mqtt_config = mqtt_config
                    .with_no_session() // Ignore any already published tokens, possibly stale.
                    .with_subscriptions(topic);

                Ok(Self::Jwt {
                    mqtt_config: Box::new(mqtt_config),
                    topic_prefix,
                })
            }
        }
    }

    pub async fn get_auth_header_value(&self) -> Result<HeaderValue, C8yAuthRetrieverError> {
        let header_value = match &self {
            Self::Basic { credentials_path } => {
                debug!("Using basic authentication.");
                let (username, password) = read_c8y_credentials(credentials_path)?;
                format!(
                    "Basic {}",
                    BASE64_STANDARD.encode(format!("{username}:{password}"))
                )
                .parse()?
            }
            Self::Jwt {
                mqtt_config,
                topic_prefix,
            } => {
                debug!("Using JWT token bearer authentication.");
                let jwt_token = Self::get_jwt_token(mqtt_config, topic_prefix).await?;
                format!("Bearer {}", jwt_token.token()).parse()?
            }
        };
        Ok(header_value)
    }

    async fn get_jwt_token(
        mqtt_config: &mqtt_channel::Config,
        topic_prefix: &TopicPrefix,
    ) -> Result<SmartRestJwtResponse, JwtError> {
        let mut mqtt_con = Connection::new(mqtt_config).await?;
        let pub_topic = format!("{}/s/uat", topic_prefix);

        tokio::time::sleep(Duration::from_millis(20)).await;
        for _ in 0..3 {
            mqtt_con
                .published
                .publish(
                    mqtt_channel::MqttMessage::new(
                        &Topic::new_unchecked(&pub_topic),
                        "".to_string(),
                    )
                    .with_qos(mqtt_channel::QoS::AtMostOnce),
                )
                .await?;
            info!("JWT token requested");

            tokio::select! {
                maybe_err = mqtt_con.errors.next() => {
                    if let Some(err) = maybe_err {
                                error!("Fail to retrieve JWT token: {err}");
                    return Err(JwtError::NoJwtReceived);
                    }
                }
                maybe_msg = tokio::time::timeout(Duration::from_secs(20), mqtt_con.received.next()) => {
                    match maybe_msg {
                        Ok(Some(msg)) => {
                            info!("JWT token received");
                            let token_smartrest = msg.payload_str()?.to_string();
                            return Ok(SmartRestJwtResponse::try_new(&token_smartrest)?);
                        }
                        Ok(None) => return Err(JwtError::NoJwtReceived),
                        Err(_elapsed) => continue,
                    }
                }
            }
        }

        error!("Fail to retrieve JWT token after 3 attempts");
        Err(JwtError::NoJwtReceived)
    }
}

pub fn read_c8y_credentials(
    credentials_path: &Utf8Path,
) -> Result<(String, String), CredentialsFileError> {
    let contents = std::fs::read_to_string(credentials_path).map_err(|e| {
        CredentialsFileError::ReadCredentialsFailed {
            context: format!(
                "Failed to read the basic auth credentials file. file={}",
                credentials_path
            )
            .to_string(),
            source: e,
        }
    })?;
    let credentials: Credentials = toml::from_str(&contents)
        .map_err(|e| CredentialsFileError::TomlError(credentials_path.into(), e))?;
    let BasicCredentials { username, password } = credentials.c8y;

    Ok((username, password))
}

#[derive(thiserror::Error, Debug)]
pub enum CredentialsFileError {
    #[error("{context}: {source}")]
    ReadCredentialsFailed {
        context: String,
        source: std::io::Error,
    },

    #[error("Error while parsing credentials file: '{0}': {1}.")]
    TomlError(PathBuf, #[source] toml::de::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum JwtError {
    #[error(transparent)]
    NoMqttConnection(#[from] mqtt_channel::MqttError),

    #[error(transparent)]
    IllFormedJwt(#[from] SmartRestDeserializerError),

    #[error("No JWT token has been received")]
    NoJwtReceived,
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test]
    fn get_url_for_get_id_returns_correct_address() {
        let c8y = C8yEndPoint::new("test_host", "test_host", ProxyUrlGenerator::default());
        let res = c8y.c8y_url_for_internal_id("test_device");

        assert_eq!(
            res,
            "https://test_host/identity/externalIds/c8y_Serial/test_device"
        );
    }

    #[test]
    fn get_url_for_sw_list_returns_correct_address() {
        let c8y = C8yEndPoint::new("test_host", "test_host", ProxyUrlGenerator::default());
        let res = c8y.c8y_url_for_sw_list("12345");

        assert_eq!(res, "https://test_host/inventory/managedObjects/12345");
    }

    #[test_case("http://aaa.test.com")]
    #[test_case("https://aaa.test.com")]
    #[test_case("ftp://aaa.test.com")]
    #[test_case("mqtt://aaa.test.com")]
    #[test_case("https://t1124124.test.com")]
    #[test_case("https://t1124124.test.com:12345")]
    #[test_case("https://t1124124.test.com/path")]
    #[test_case("https://t1124124.test.com/path/to/file.test")]
    #[test_case("https://t1124124.test.com/path/to/file")]
    #[test_case("https://t1124124.mqtt-url.com/path/to/file")]
    fn url_is_my_tenant_correct_urls(url: &str) {
        let c8y = C8yEndPoint::new(
            "test.test.com",
            "test.mqtt-url.com",
            ProxyUrlGenerator::default(),
        );
        assert_eq!(c8y.maybe_tenant_url(url), Some(url.parse().unwrap()));
    }

    #[test_case("http://aaa.test.com")]
    #[test_case("https://aaa.test.com")]
    #[test_case("ftp://aaa.test.com")]
    #[test_case("mqtt://aaa.test.com")]
    #[test_case("https://t1124124.test.com")]
    #[test_case("https://t1124124.test.com:12345")]
    #[test_case("https://t1124124.test.com/path")]
    #[test_case("https://t1124124.test.com/path/to/file.test")]
    #[test_case("https://t1124124.test.com/path/to/file")]
    #[test_case("https://t1124124.mqtt-url.com/path/to/file")]
    fn url_is_my_tenant_correct_urls_with_http_port(url: &str) {
        let c8y = C8yEndPoint::new(
            "test.test.com:443",
            "test.mqtt-url.com",
            ProxyUrlGenerator::default(),
        );
        assert_eq!(c8y.maybe_tenant_url(url), Some(url.parse().unwrap()));
    }

    #[test_case("http://aaa.test.com")]
    #[test_case("https://aaa.test.com")]
    #[test_case("ftp://aaa.test.com")]
    #[test_case("mqtt://aaa.test.com")]
    #[test_case("https://t1124124.test.com")]
    #[test_case("https://t1124124.test.com:12345")]
    #[test_case("https://t1124124.test.com/path")]
    #[test_case("https://t1124124.test.com/path/to/file.test")]
    #[test_case("https://t1124124.test.com/path/to/file")]
    #[test_case("https://t1124124.mqtt-url.com/path/to/file")]
    fn url_is_my_tenant_correct_urls_with_mqtt_port(url: &str) {
        let c8y = C8yEndPoint::new(
            "test.test.com",
            "test.mqtt-url.com:8883",
            ProxyUrlGenerator::default(),
        );
        assert_eq!(c8y.maybe_tenant_url(url), Some(url.parse().unwrap()));
    }

    #[test_case("test.com")]
    #[test_case("http://test.co")]
    #[test_case("http://test.co.te")]
    #[test_case("http://test.com:123456")]
    #[test_case("http://test.com::12345")]
    #[test_case("http://localhost")]
    #[test_case("http://abc.com")]
    fn url_is_my_tenant_incorrect_urls(url: &str) {
        let c8y = C8yEndPoint::new(
            "test.test.com",
            "test.mqtt-url.com",
            ProxyUrlGenerator::default(),
        );
        assert!(c8y.maybe_tenant_url(url).is_none());
    }

    #[test]
    fn url_is_my_tenant_with_hostname_without_commas() {
        let c8y = C8yEndPoint::new(
            "custom-domain",
            "non-custom-mqtt-domain",
            ProxyUrlGenerator::default(),
        );
        let url = "http://custom-domain/path";
        assert_eq!(c8y.maybe_tenant_url(url), Some(url.parse().unwrap()));
    }

    #[test]
    fn url_is_not_my_tenant_with_hostname_without_commas() {
        let c8y = C8yEndPoint::new(
            "custom-domain",
            "non-custom-mqtt-domain",
            ProxyUrlGenerator::default(),
        );
        let url = "http://unrelated-domain/path";
        assert!(c8y.maybe_tenant_url(url).is_none());
    }

    #[ignore = "Until #2804 is fixed"]
    #[test]
    fn url_is_my_tenant_check_not_too_broad() {
        let c8y = C8yEndPoint::new("abc.com", "abc.com", ProxyUrlGenerator::default());
        dbg!(c8y.maybe_tenant_url("http://xyz.com"));
        assert!(c8y.maybe_tenant_url("http://xyz.com").is_none());
    }

    #[test_case("http://aaa.test.com", "https://127.0.0.1:1234/c8y/")]
    #[test_case("https://aaa.test.com", "https://127.0.0.1:1234/c8y/")]
    #[test_case("http://aaa.unrelated.com", "http://aaa.unrelated.com/")] // Unchanged: unrelated tenant
    #[test_case("ftp://aaa.test.com", "ftp://aaa.test.com/")] // Unchanged: unrelated protocol
    #[test_case("https://t1124124.test.com", "https://127.0.0.1:1234/c8y/")]
    #[test_case("https://t1124124.test.com:12345", "https://127.0.0.1:1234/c8y/")]
    #[test_case("https://t1124124.test.com/path", "https://127.0.0.1:1234/c8y/path")]
    #[test_case(
        "https://t1124124.test.com/path/to/file.test",
        "https://127.0.0.1:1234/c8y/path/to/file.test"
    )]
    #[test_case(
        "https://t1124124.test.com/path/to/file",
        "https://127.0.0.1:1234/c8y/path/to/file"
    )]
    #[test_case(
        "https://t1124124.mqtt-url.com/path/to/file",
        "https://127.0.0.1:1234/c8y/path/to/file"
    )]
    fn local_proxy_url(url: &str, proxy_url: &str) {
        let c8y = C8yEndPoint::new(
            "test.test.com",
            "test.mqtt-url.com",
            ProxyUrlGenerator::new("127.0.0.1".into(), 1234, Protocol::Https),
        );
        assert_eq!(c8y.local_proxy_url(url).unwrap().as_ref(), proxy_url);
    }
}
