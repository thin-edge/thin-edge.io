use crate::smartrest::error::SmartRestDeserializerError;
use crate::smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_channel::Connection;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use reqwest::Url;
use std::time::Duration;
use tedge_config::mqtt_config::MqttConfigBuildError;
use tedge_config::TEdgeConfig;
use tracing::error;
use tracing::info;

/// Define a C8y endpoint
#[derive(Debug)]
pub struct C8yEndPoint {
    pub c8y_host: String,
    pub device_id: String,
    pub c8y_internal_id: String,
}

impl C8yEndPoint {
    pub fn new(c8y_host: &str, device_id: &str, c8y_internal_id: &str) -> C8yEndPoint {
        C8yEndPoint {
            c8y_host: c8y_host.into(),
            device_id: device_id.into(),
            c8y_internal_id: c8y_internal_id.into(),
        }
    }

    pub fn get_c8y_internal_id(&self) -> &str {
        &self.c8y_internal_id
    }

    pub fn set_c8y_internal_id(&mut self, id: String) {
        self.c8y_internal_id = id;
    }

    fn get_base_url(&self) -> String {
        let mut url_get_id = String::new();
        if !self.c8y_host.starts_with("http") {
            url_get_id.push_str("https://");
        }
        url_get_id.push_str(&self.c8y_host);

        url_get_id
    }

    pub fn get_url_for_sw_list(&self) -> String {
        let mut url_update_swlist = self.get_base_url();
        url_update_swlist.push_str("/inventory/managedObjects/");
        url_update_swlist.push_str(&self.c8y_internal_id);

        url_update_swlist
    }

    pub fn get_url_for_get_id(&self, device_id: Option<&str>) -> String {
        let mut url_get_id = self.get_base_url();
        url_get_id.push_str("/identity/externalIds/c8y_Serial/");
        url_get_id.push_str(device_id.unwrap_or(&self.device_id));

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

    pub fn url_is_in_my_tenant_domain(&self, url: &str) -> bool {
        // c8y URL may contain either `Tenant Name` or Tenant Id` so they can be one of following options:
        // * <tenant_name>.<domain> eg: sample.c8y.io
        // * <tenant_id>.<domain> eg: t12345.c8y.io
        // These URLs may be both equivalent and point to the same tenant.
        // We are going to remove that and only check if the domain is the same.
        let tenant_uri = &self.c8y_host;
        let url_host = match Url::parse(url) {
            Ok(url) => match url.host() {
                Some(host) => host.to_string(),
                None => return false,
            },
            Err(_err) => {
                return false;
            }
        };

        let url_domain = url_host.splitn(2, '.').collect::<Vec<&str>>();
        let tenant_domain = tenant_uri.splitn(2, '.').collect::<Vec<&str>>();
        if url_domain.get(1) == tenant_domain.get(1) {
            return true;
        }
        false
    }
}

pub struct C8yMqttJwtTokenRetriever {
    mqtt_config: mqtt_channel::Config,
}

impl C8yMqttJwtTokenRetriever {
    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Result<Self, MqttConfigBuildError> {
        let mqtt_config = tedge_config.mqtt_config()?;

        Ok(Self::new(mqtt_config))
    }

    pub fn new(mqtt_config: mqtt_channel::Config) -> Self {
        let topic = TopicFilter::new_unchecked("c8y/s/dat");
        let mqtt_config = mqtt_config
            .with_no_session() // Ignore any already published tokens, possibly stale.
            .with_subscriptions(topic);

        C8yMqttJwtTokenRetriever { mqtt_config }
    }

    pub async fn get_jwt_token(&mut self) -> Result<SmartRestJwtResponse, JwtError> {
        let mut mqtt_con = Connection::new(&self.mqtt_config).await?;

        tokio::time::sleep(Duration::from_millis(20)).await;
        for _ in 0..3 {
            mqtt_con
                .published
                .publish(mqtt_channel::Message::new(
                    &Topic::new_unchecked("c8y/s/uat"),
                    "".to_string(),
                ))
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
        let c8y = C8yEndPoint::new("test_host", "test_device", "internal-id");
        let res = c8y.get_url_for_get_id(None);

        assert_eq!(
            res,
            "https://test_host/identity/externalIds/c8y_Serial/test_device"
        );
    }

    #[test]
    fn get_url_for_sw_list_returns_correct_address() {
        let c8y = C8yEndPoint::new("test_host", "test_device", "12345");
        let res = c8y.get_url_for_sw_list();

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
    fn url_is_my_tenant_correct_urls(url: &str) {
        let c8y = C8yEndPoint::new("test.test.com", "test_device", "internal-id");
        assert!(c8y.url_is_in_my_tenant_domain(url));
    }

    #[test_case("test.com")]
    #[test_case("http://test.co")]
    #[test_case("http://test.co.te")]
    #[test_case("http://test.com:123456")]
    #[test_case("http://test.com::12345")]
    fn url_is_my_tenant_incorrect_urls(url: &str) {
        let c8y = C8yEndPoint::new("test.test.com", "test_device", "internal-id");
        assert!(!c8y.url_is_in_my_tenant_domain(url));
    }
}
