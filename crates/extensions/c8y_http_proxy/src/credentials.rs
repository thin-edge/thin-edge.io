use async_trait::async_trait;
use c8y_api::http_proxy::C8yMqttJwtTokenRetriever;
use c8y_api::http_proxy::JwtError;
use tedge_actors::ClientMessageBox;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

pub type JwtRequest = ();
pub type JwtResult = Result<String, JwtError>;

/// Retrieves JWT tokens authenticating the device
pub type JwtRetriever = ClientMessageBox<JwtRequest, JwtResult>;

/// A JwtRetriever that gets JWT tokens from C8Y over MQTT
pub struct C8YJwtRetriever {
    mqtt_retriever: C8yMqttJwtTokenRetriever,
}

impl C8YJwtRetriever {
    pub fn builder(
        mqtt_config: mqtt_channel::Config,
    ) -> ServerActorBuilder<C8YJwtRetriever, Sequential> {
        let mqtt_retriever = C8yMqttJwtTokenRetriever::new(mqtt_config);
        let server = C8YJwtRetriever { mqtt_retriever };
        ServerActorBuilder::new(server, &ServerConfig::default(), Sequential)
    }
}

#[async_trait]
impl Server for C8YJwtRetriever {
    type Request = JwtRequest;
    type Response = JwtResult;

    fn name(&self) -> &str {
        "C8YJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        let response = self.mqtt_retriever.get_jwt_token().await?;
        Ok(response.token())
    }
}

/// A JwtRetriever that simply always returns the same JWT token (possibly none)
pub(crate) struct ConstJwtRetriever {
    pub token: String,
}

#[async_trait]
impl Server for ConstJwtRetriever {
    type Request = JwtRequest;
    type Response = JwtResult;

    fn name(&self) -> &str {
        "ConstJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        Ok(self.token.clone())
    }
}
