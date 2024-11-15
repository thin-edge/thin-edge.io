use crate::actor::C8YHttpProxyActor;
use c8y_api::proxy_url::ProxyUrlGenerator;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_actors::Service;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;

mod actor;
pub mod handle;
pub mod messages;

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
