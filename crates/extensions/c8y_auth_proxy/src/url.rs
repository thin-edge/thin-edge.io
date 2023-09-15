use std::net::IpAddr;

use miette::Context;
use miette::IntoDiagnostic;
use tedge_config::TEdgeConfig;

pub struct ProxyUrlGenerator {
    address: IpAddr,
    port: u16,
}

impl ProxyUrlGenerator {
    pub fn new(address: IpAddr, port: u16) -> Self {
        Self { address, port }
    }

    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Self {
        Self {
            address: tedge_config.c8y.proxy.bind.address,
            port: tedge_config.c8y.proxy.bind.port,
        }
    }

    pub fn proxy_url(&self, mut cumulocity_url: url::Url) -> url::Url {
        cumulocity_url.set_ip_host(self.address).unwrap();
        cumulocity_url.set_scheme("http").unwrap();
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
            address: [127, 0, 0, 1].into(),
            port: 8001,
        };

        assert_eq!(
            url_gen
                .proxy_url("https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects")
                .unwrap()
                .to_string(),
            "http://127.0.0.1:8001/c8y/inventory/managedObjects"
        )
    }
}
