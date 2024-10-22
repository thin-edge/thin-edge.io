use crate::smartrest::error::SmartRestDeserializerError;
use crate::smartrest::smartrest_deserializer::SmartRestJwtResponse;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use mqtt_channel::Connection;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::InvalidHeaderValue;
use reqwest::Url;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tedge_config::mqtt_config::MqttConfigBuildError;
use tedge_config::MultiError;
use tedge_config::TEdgeConfig;
use tedge_config::TopicPrefix;
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
    pub device_id: String,
    pub headers: HeaderMap,
    devices_internal_id: HashMap<String, String>,
}

impl C8yEndPoint {
    pub fn new(c8y_host: &str, c8y_mqtt_host: &str, device_id: &str) -> C8yEndPoint {
        C8yEndPoint {
            c8y_host: c8y_host.into(),
            c8y_mqtt_host: c8y_mqtt_host.into(),
            device_id: device_id.into(),
            headers: HeaderMap::new(),
            devices_internal_id: HashMap::new(),
        }
    }

    pub fn get_internal_id(&self, device_id: String) -> Result<String, C8yEndPointError> {
        match self.devices_internal_id.get(&device_id) {
            Some(internal_id) => Ok(internal_id.to_string()),
            None => Err(C8yEndPointError::InternalIdNotFound(device_id.clone())),
        }
    }

    pub fn set_internal_id(&mut self, device_id: String, internal_id: String) {
        self.devices_internal_id.insert(device_id, internal_id);
    }

    fn get_base_url(&self) -> String {
        let mut url_get_id = String::new();
        if !self.c8y_host.starts_with("http") {
            url_get_id.push_str("https://");
        }
        url_get_id.push_str(&self.c8y_host);

        url_get_id
    }

    pub fn get_url_for_sw_list(&self, internal_id: String) -> String {
        let mut url_update_swlist = self.get_base_url();
        url_update_swlist.push_str("/inventory/managedObjects/");
        url_update_swlist.push_str(&internal_id);
        url_update_swlist
    }

    pub fn get_url_for_internal_id(&self, device_id: &str) -> String {
        let mut url_get_id = self.get_base_url();
        url_get_id.push_str("/identity/externalIds/c8y_Serial/");
        url_get_id.push_str(device_id);

        url_get_id
    }

    pub fn get_url_for_create_event(&self) -> String {
        let mut url_create_event = self.get_base_url();
        url_create_event.push_str("/event/events/");

        url_create_event
    }

    pub fn get_url_for_event_binary_upload(&self, event_id: &str) -> String {
        let mut url_event_binary = self.get_url_for_create_event();
        url_event_binary.push_str(event_id);
        url_event_binary.push_str("/binaries");

        url_event_binary
    }

    pub fn get_url_for_event_binary_upload_unchecked(&self, event_id: &str) -> Url {
        let url = self.get_url_for_event_binary_upload(event_id);
        Url::parse(&url).unwrap()
    }

    pub fn maybe_tenant_url(&self, url: &str) -> Option<Url> {
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

        let (_, host) = url_host.split_once('.').unwrap_or((url_host, ""));
        let (_, c8y_http_host) = tenant_http_host
            .split_once('.')
            .unwrap_or((tenant_http_host, ""));
        let (_, c8y_mqtt_host) = tenant_mqtt_host
            .split_once('.')
            .unwrap_or((tenant_mqtt_host, ""));

        // The configured `c8y.http` setting may have a port value specified,
        // but the incoming URL is less likely to have any port specified.
        // Hence just matching the host prefix.
        (host == c8y_http_host || host == c8y_mqtt_host).then_some(url)
    }
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
    MqttConfigBuild(#[from] MqttConfigBuildError),

    #[error(transparent)]
    ConfigMulti(#[from] MultiError),

    #[error(transparent)]
    JwtError(#[from] JwtError),

    #[error(transparent)]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error(transparent)]
    CredentialsFileError(#[from] CredentialsFileError),
}

impl C8yAuthRetriever {
    pub fn from_tedge_config(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yAuthRetrieverError> {
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let topic_prefix = c8y_config.bridge.topic_prefix.clone();

        if c8y_config.use_basic_auth {
            Ok(Self::Basic {
                credentials_path: c8y_config.credentials_path.clone(),
            })
        } else {
            let mqtt_config = tedge_config
                .mqtt_config()
                .map_err(MqttConfigBuildError::from)?;

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

    pub async fn get_auth_header_value(&mut self) -> Result<HeaderValue, C8yAuthRetrieverError> {
        let header_value = match &self {
            Self::Basic { credentials_path } => {
                let (username, password) = read_c8y_credentials(credentials_path)?;
                format!("Basic {}", base64::encode(format!("{username}:{password}"))).parse()?
            }
            Self::Jwt {
                mqtt_config,
                topic_prefix,
            } => {
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
            context: "Failed to read the basic auth credentials file.".to_string(),
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
        let c8y = C8yEndPoint::new("test_host", "test_host", "test_device");
        let res = c8y.get_url_for_internal_id("test_device");

        assert_eq!(
            res,
            "https://test_host/identity/externalIds/c8y_Serial/test_device"
        );
    }

    #[test]
    fn get_url_for_sw_list_returns_correct_address() {
        let mut c8y = C8yEndPoint::new("test_host", "test_host", "test_device");
        c8y.devices_internal_id
            .insert("test_device".to_string(), "12345".to_string());
        let internal_id = c8y.get_internal_id("test_device".to_string()).unwrap();
        let res = c8y.get_url_for_sw_list(internal_id);

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
        let c8y = C8yEndPoint::new("test.test.com", "test.mqtt-url.com", "test_device");
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
        let c8y = C8yEndPoint::new("test.test.com:443", "test.mqtt-url.com", "test_device");
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
        let c8y = C8yEndPoint::new("test.test.com", "test.mqtt-url.com:8883", "test_device");
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
        let c8y = C8yEndPoint::new("test.test.com", "test.mqtt-url.com", "test_device");
        assert!(c8y.maybe_tenant_url(url).is_none());
    }

    #[test]
    fn url_is_my_tenant_with_hostname_without_commas() {
        let c8y = C8yEndPoint::new("custom-domain", "non-custom-mqtt-domain", "test_device");
        let url = "http://custom-domain/path";
        assert_eq!(c8y.maybe_tenant_url(url), Some(url.parse().unwrap()));
    }

    #[ignore = "Until #2804 is fixed"]
    #[test]
    fn url_is_my_tenant_check_not_too_broad() {
        let c8y = C8yEndPoint::new("abc.com", "abc.com", "test_device");
        dbg!(c8y.maybe_tenant_url("http://xyz.com"));
        assert!(c8y.maybe_tenant_url("http://xyz.com").is_none());
    }

    #[test]
    fn check_non_cached_internal_id_for_a_device() {
        let mut c8y = C8yEndPoint::new("test_host", "test_host", "test_device");
        c8y.devices_internal_id
            .insert("test_device".to_string(), "12345".to_string());
        let end_pt_err = c8y.get_internal_id("test_child".into()).unwrap_err();

        assert_eq!(
            end_pt_err.to_string(),
            "Cumulocity internal id not found for the device: test_child".to_string()
        );
    }
}
