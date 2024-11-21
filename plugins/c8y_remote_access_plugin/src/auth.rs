use c8y_api::http_proxy::C8yAuthRetriever;
use http::HeaderValue;
use miette::IntoDiagnostic;
use tedge_config::TEdgeConfig;

pub struct Auth(HeaderValue);

impl Auth {
    pub fn authorization_header(&self) -> HeaderValue {
        self.0.clone()
    }

    pub async fn retrieve(config: &TEdgeConfig, c8y_profile: Option<&str>) -> miette::Result<Auth> {
        let retriever =
            C8yAuthRetriever::from_tedge_config(config, c8y_profile).into_diagnostic()?;

        retriever
            .get_auth_header_value()
            .await
            .map(Auth)
            .into_diagnostic()
    }
}
