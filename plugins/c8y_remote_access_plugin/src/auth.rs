use c8y_api::http_proxy::C8yAuthRetriever;
use http::HeaderValue;
use miette::IntoDiagnostic;
use tedge_config::tedge_toml::mapper_config::C8yMapperConfig;
use tedge_config::TEdgeConfig;

pub struct Auth(HeaderValue);

impl Auth {
    pub fn authorization_header(&self) -> HeaderValue {
        self.0.clone()
    }

    pub async fn retrieve(
        config: &TEdgeConfig,
        c8y_config: &C8yMapperConfig,
    ) -> miette::Result<Auth> {
        let retriever =
            C8yAuthRetriever::from_tedge_config(config, c8y_config).into_diagnostic()?;

        retriever
            .get_auth_header_value()
            .await
            .map(Auth)
            .into_diagnostic()
    }

    #[cfg(test)]
    pub fn test_value(test_value: HeaderValue) -> Self {
        Self(test_value)
    }
}
