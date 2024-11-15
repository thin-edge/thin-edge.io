use crate::actor::C8YHttpProxyActor;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_actors::Service;
use tedge_config::ConfigNotSet;
use tedge_config::MultiError;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod handle;
pub mod messages;

use c8y_api::proxy_url::Protocol;
use c8y_api::proxy_url::ProxyUrlGenerator;
pub use http::HeaderMap;

#[cfg(test)]
mod tests;

/// Configuration of C8Y REST API
#[derive(Clone)]
pub struct C8YHttpConfig {
    pub c8y_http_host: String,
    pub c8y_mqtt_host: String,
    pub device_id: String,
    retry_interval: Duration,
    proxy: ProxyUrlGenerator,
}

impl C8YHttpConfig {
    pub fn new(
        device_id: String,
        c8y_http_host: String,
        c8y_mqtt_host: String,
        proxy: ProxyUrlGenerator,
    ) -> Self {
        C8YHttpConfig {
            c8y_http_host,
            c8y_mqtt_host,
            device_id,
            retry_interval: Duration::from_secs(5),
            proxy,
        }
    }

    pub fn try_new(
        tedge_config: &TEdgeConfig,
        c8y_profile: Option<&str>,
    ) -> Result<Self, C8yHttpConfigBuildError> {
        let c8y_http_host = tedge_config
            .c8y
            .try_get(c8y_profile)?
            .http
            .or_config_not_set()?
            .to_string();
        let c8y_mqtt_host = tedge_config
            .c8y
            .try_get(c8y_profile)?
            .mqtt
            .or_config_not_set()?
            .to_string();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let retry_interval = Duration::from_secs(5);

        // Temporary code: this will be deprecated along c8y_http_proxy
        let c8y_config = tedge_config.c8y.try_get(c8y_profile)?;
        let auth_proxy_addr = c8y_config.proxy.client.host.clone();
        let auth_proxy_port = c8y_config.proxy.client.port;
        let auth_proxy_protocol = c8y_config
            .proxy
            .cert_path
            .or_none()
            .map_or(Protocol::Http, |_| Protocol::Https);
        let proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, auth_proxy_protocol);

        Ok(Self {
            c8y_http_host,
            c8y_mqtt_host,
            proxy,
            device_id,
            retry_interval,
        })
    }
}

/// The errors that could occur while building `C8YHttpConfig` struct.
#[derive(Debug, thiserror::Error)]
pub enum C8yHttpConfigBuildError {
    #[error(transparent)]
    FromReadError(#[from] ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    FromMultiError(#[from] MultiError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A proxy to C8Y REST API
pub struct C8YHttpProxyBuilder {
    /// Config
    config: C8YHttpConfig,

    /// Connection to an HTTP actor
    http: ClientMessageBox<HttpRequest, HttpResult>,
}

impl C8YHttpProxyBuilder {
    pub fn new(config: C8YHttpConfig, http: &mut impl Service<HttpRequest, HttpResult>) -> Self {
        let http = ClientMessageBox::new(http);
        C8YHttpProxyBuilder { config, http }
    }

    pub fn build(self) -> C8YHttpProxyActor {
        C8YHttpProxyActor::new(self.config, self.http)
    }
}
