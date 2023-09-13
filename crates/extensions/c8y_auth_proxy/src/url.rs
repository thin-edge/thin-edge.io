use miette::Context;
use miette::IntoDiagnostic;
use tedge_config::TEdgeConfig;

pub struct ProxyUrlGenerator {
    port: u16,
}

impl ProxyUrlGenerator {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn from_tedge_config(tedge_config: &TEdgeConfig) -> Self {
        Self {
            port: tedge_config.c8y.proxy.bind.port,
        }
    }

    pub fn proxy_url(&self, cumulocity_url: &str) -> miette::Result<url::Url> {
        let mut parsed_url = cumulocity_url
            .parse::<url::Url>()
            .into_diagnostic()
            .wrap_err_with(|| format!("failed to parse url: {cumulocity_url}"))?;
        parsed_url
            .set_host(Some("localhost"))
            .into_diagnostic()
            .wrap_err("failed to update host of url")?;
        parsed_url.set_scheme("http").unwrap();
        parsed_url.set_port(Some(self.port)).unwrap();
        Ok(parsed_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_generates_proxy_urls_for_the_provided_port() {
        let url_gen = ProxyUrlGenerator { port: 8001 };

        assert_eq!(
            url_gen
                .proxy_url("https://thin-edge-io.eu-latest.cumulocity.com/inventory/managedObjects")
                .unwrap()
                .to_string(),
            "http://localhost:8001/inventory/managedObjects"
        )
    }
}
