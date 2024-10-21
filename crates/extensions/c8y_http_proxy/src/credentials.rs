use async_trait::async_trait;
use c8y_api::http_proxy::C8yMqttJwtTokenRetriever;
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

/// A JwtRetriever that gets JWT tokens from C8Y over MQTT and returns authorization header
pub struct C8YJwtRetriever {
    mqtt_retriever: C8yMqttJwtTokenRetriever,
}

impl C8YJwtRetriever {
    pub fn builder(
        mqtt_config: mqtt_channel::Config,
        topic_prefix: TopicPrefix,
    ) -> ServerActorBuilder<C8YJwtRetriever, Sequential> {
        let mqtt_retriever = C8yMqttJwtTokenRetriever::new(mqtt_config, topic_prefix);
        let server = C8YJwtRetriever { mqtt_retriever };
        ServerActorBuilder::new(server, &ServerConfig::default(), Sequential)
    }
}

#[async_trait]
impl Server for C8YJwtRetriever {
    type Request = HttpHeaderRequest;
    type Response = HttpHeaderResult;

    fn name(&self) -> &str {
        "C8YJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let mut heeader_map = HeaderMap::new();
        let response = self.mqtt_retriever.get_jwt_token().await?;
        heeader_map.insert(
            AUTHORIZATION,
            format!("Bearer {}", response.token()).parse()?,
        );
        Ok(heeader_map)
    }
}

/// Return base64 encoded Basic Auth header
pub struct C8YBasicAuthRetriever {
    username: String,
    password: String,
}

impl C8YBasicAuthRetriever {
    pub fn builder(
        username: &str,
        password: &str,
    ) -> ServerActorBuilder<C8YBasicAuthRetriever, Sequential> {
        let server = C8YBasicAuthRetriever {
            username: username.into(),
            password: password.into(),
        };
        ServerActorBuilder::new(server, &ServerConfig::default(), Sequential)
    }
}

#[async_trait]
impl Server for C8YBasicAuthRetriever {
    type Request = HttpHeaderRequest;
    type Response = HttpHeaderResult;

    fn name(&self) -> &str {
        "C8YBasicAuthRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let mut header_map = HeaderMap::new();
        header_map.insert(
            AUTHORIZATION,
            format!(
                "Basic {}",
                base64::encode(format!("{}:{}", self.username, self.password))
            )
            .parse()?,
        );
        Ok(header_map)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum HttpHeaderError {
    #[error(transparent)]
    JwtError(#[from] c8y_api::http_proxy::JwtError),

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
