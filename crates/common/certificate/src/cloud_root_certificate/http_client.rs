#![allow(clippy::disallowed_types)]

//! Common configuration of HTTP clients for thin-edge.io components.
//!
//! There are some common configuration that we want to share in all thin-edge components that
//! perform HTTP operations, like known certificates, but also some components might want to set
//! some additional fields (like UA or identity). In this module we preset common configuration,
//! allowing components to opt-out or opt-in to other config options.

use reqwest::Identity;

use super::CloudHttpConfig;

pub const USER_AGENT: &str = concat!("thin-edge.io ", env!("CARGO_PKG_VERSION"));

impl CloudHttpConfig {
    pub fn client_builder(&self) -> HttpClientBuilder {
        let builder = self
            .certificates
            .iter()
            .cloned()
            .fold(reqwest::ClientBuilder::new(), |builder, cert| {
                builder.add_root_certificate(cert)
            });

        let builder = if let Some(proxy) = self.proxy.clone() {
            builder.proxy(proxy)
        } else {
            builder.no_proxy()
        };

        HttpClientBuilder {
            builder,
            user_agent: Some(USER_AGENT),
        }
    }
}

/// A wrapper for reqwest::ClientBuilder, used primarily becase reqwest::ClientBuilder doesn't allow
/// unsetting some fields that were previously set.
pub struct HttpClientBuilder {
    builder: reqwest::ClientBuilder,
    user_agent: Option<&'static str>,
}

impl HttpClientBuilder {
    pub fn identity(self, identity: Identity) -> Self {
        Self {
            builder: self.builder.identity(identity),
            ..self
        }
    }

    pub fn set_user_agent(mut self, should_set: bool) -> Self {
        if should_set {
            self.user_agent = Some(USER_AGENT);
        } else {
            self.user_agent = None;
        };
        self
    }

    pub fn build(self) -> reqwest::Result<reqwest::Client> {
        let builder = self.builder;
        let builder = match self.user_agent {
            Some(ua) => builder.user_agent(ua),
            None => builder,
        };
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;

    #[tokio::test]
    #[test_case::test_case(None, StatusCode::OK)]
    #[test_case::test_case(Some(true), StatusCode::OK)]
    #[test_case::test_case(Some(false), StatusCode::NOT_IMPLEMENTED)]
    async fn client_builder_sets_user_agent_by_default(
        should_set_ua: Option<bool>,
        expected_status: StatusCode,
    ) {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/")
            .match_header("user-agent", USER_AGENT)
            .with_status(200)
            .create_async()
            .await;
        let builder = CloudHttpConfig::test_value().client_builder();

        let builder = if let Some(should_set_ua) = should_set_ua {
            builder.set_user_agent(should_set_ua)
        } else {
            builder
        };

        let client = builder.build().unwrap();
        let response = client.get(server.url()).send().await.unwrap();

        assert_eq!(response.status(), expected_status);
    }
}
