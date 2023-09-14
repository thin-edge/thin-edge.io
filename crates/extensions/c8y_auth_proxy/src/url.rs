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

    pub fn proxy_url(&self, cumulocity_url: &str) -> miette::Result<url::Url> {
        let mut parsed_url = cumulocity_url
            .parse::<url::Url>()
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to parse url: {cumulocity_url}"))?;
        parsed_url.set_ip_host(self.address).unwrap();
        parsed_url.set_scheme("http").unwrap();
        parsed_url.set_port(Some(self.port)).unwrap();
        parsed_url.set_path(&format!("/c8y{}", parsed_url.path()));
        Ok(parsed_url)
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
