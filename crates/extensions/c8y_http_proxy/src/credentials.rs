use async_trait::async_trait;
use c8y_api::http_proxy::C8yAuthRetriever;
use c8y_api::http_proxy::C8yAuthType;
use http::header::AUTHORIZATION;
use http::HeaderMap;
use tedge_actors::ClientMessageBox;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;
use tedge_config::TopicPrefix;

pub type HttpHeaderRequest = ();
pub type HttpHeaderResult = Result<HeaderMap, HttpHeaderError>;

/// Retrieves HTTP headers
pub type HttpHeaderRetriever = ClientMessageBox<HttpHeaderRequest, HttpHeaderResult>;

/// An HTTP header retriever
pub struct C8YHeaderRetriever {
    auth_retriever: C8yAuthRetriever,
}

impl C8YHeaderRetriever {
    pub fn builder(
        auth: C8yAuthType,
        topic_prefix: TopicPrefix,
    ) -> ServerActorBuilder<C8YHeaderRetriever, Sequential> {
        let auth_retriever = match auth {
            C8yAuthType::JwtToken { mqtt_config } => {
                C8yAuthRetriever::new_with_jwt_auth(*mqtt_config, topic_prefix)
            }
            C8yAuthType::Basic { credentials_path } => {
                C8yAuthRetriever::new_with_basic_auth(credentials_path, topic_prefix)
            }
        };
        let server = C8YHeaderRetriever { auth_retriever };
        ServerActorBuilder::new(server, &ServerConfig::default(), Sequential)
    }
}

#[async_trait]
impl Server for C8YHeaderRetriever {
    type Request = HttpHeaderRequest;
    type Response = HttpHeaderResult;

    fn name(&self) -> &str {
        "C8YHeaderRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let mut header_map = HeaderMap::new();
        let auth_value = self.auth_retriever.get_auth_header_value().await?;
        header_map.insert(AUTHORIZATION, auth_value);
        Ok(header_map)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HttpHeaderError {
    #[error(transparent)]
    C8yAuthRetrieverError(#[from] c8y_api::http_proxy::C8yAuthRetrieverError),

    #[error(transparent)]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
}

/// A JwtRetriever that simply always returns the same JWT token (possibly none)
#[cfg(test)]
pub(crate) struct ConstJwtRetriever {
    pub token: String,
}

#[async_trait]
#[cfg(test)]
impl Server for ConstJwtRetriever {
    type Request = HttpHeaderRequest;
    type Response = HttpHeaderResult;

    fn name(&self) -> &str {
        "ConstJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let mut header_map = HeaderMap::new();
        header_map.insert(AUTHORIZATION, format!("Bearer {}", self.token).parse()?);
        Ok(header_map)
    }
}
