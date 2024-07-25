use c8y_api::http_proxy::C8yMqttJwtTokenRetriever;
use miette::IntoDiagnostic;
use tedge_config::TEdgeConfig;

pub struct Jwt(String);

impl Jwt {
    pub fn authorization_header(&self) -> String {
        let use_legacy_auth = std::env::var("C8Y_DEVICE_USER").is_ok()
            && std::env::var("C8Y_DEVICE_PASSWORD").is_ok();
        if use_legacy_auth {
            format!(
                "Basic {}",
                base64::encode(format!(
                    "{}:{}",
                    std::env::var("C8Y_DEVICE_USER").unwrap(),
                    std::env::var("C8Y_DEVICE_PASSWORD").unwrap()
                ))
            )
        } else {
            format!("Bearer {}", self.0)
        }
    }

    pub async fn retrieve(config: &TEdgeConfig) -> miette::Result<Jwt> {
        let mut retriever =
            C8yMqttJwtTokenRetriever::from_tedge_config(config).into_diagnostic()?;

        retriever
            .get_jwt_token()
            .await
            .map(|resp| Jwt(resp.token()))
            .into_diagnostic()
    }
}
