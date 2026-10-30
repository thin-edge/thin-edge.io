use c8y_api::domain_names::DomainNames;
use c8y_api::proxy_url::ProxyUrlGenerator;

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
    proxy: ProxyUrlGenerator,
    domain_names: DomainNames,
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
            proxy,
            domain_names: DomainNames::default(),
        }
    }

    /// Routes URLs on these domains through the local proxy, like `c8y.http`
    pub fn with_domain_names(self, domain_names: DomainNames) -> Self {
        C8YHttpConfig {
            domain_names,
            ..self
        }
    }
}
