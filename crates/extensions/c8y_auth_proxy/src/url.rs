use std::sync::Arc;

use tedge_config::TEdgeConfig;

pub struct ProxyUrlGenerator {
    host: Arc<str>,
    port: u16,
}

impl ProxyUrlGenerator {
    pub fn new(host: Arc<str>, port: u16) -> Self {
        Self { host, port }
    }

    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Self {
        Self {
            host: tedge_config.c8y.proxy.client.host.clone(),
            port: tedge_config.c8y.proxy.client.port,
        }
    }

    pub fn proxy_url(&self, mut cumulocity_url: url::Url) -> url::Url {
        cumulocity_url.set_host(Some(&self.host)).unwrap();
        cumulocity_url.set_scheme("https").unwrap();
        cumulocity_url.set_port(Some(self.port)).unwrap();
        cumulocity_url.set_path(&format!("/c8y{}", cumulocity_url.path()));
        cumulocity_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_generates_proxy_urls_for_the_provided_port() {
        let url_gen = ProxyUrlGenerator {
            host: "127.0.0.1".into(),
            port: 8001,
        };

        let url = url::Url::parse(
            "https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects",
        )
        .unwrap();

        assert_eq!(
            url_gen.proxy_url(url).to_string(),
            "https://127.0.0.1:8001/c8y/inventory/managedObjects"
        )
    }
}
