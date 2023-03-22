use async_trait::async_trait;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_channel::Connection;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::time::Duration;
use tedge_actors::ClientMessageBox;
use tedge_actors::Sequential;
use tedge_actors::Server;
use tedge_actors::ServerActorBuilder;
use tedge_actors::ServerConfig;

pub type JwtRequest = ();
pub type JwtResult = Result<String, SmartRestDeserializerError>;

/// Retrieves JWT tokens authenticating the device
pub type JwtRetriever = ClientMessageBox<JwtRequest, JwtResult>;

/// A JwtRetriever that gets JWT tokens from C8Y over MQTT
pub struct C8YJwtRetriever {
    mqtt_config: mqtt_channel::Config,
}

impl C8YJwtRetriever {
    pub fn builder(
        mqtt_config: mqtt_channel::Config,
    ) -> ServerActorBuilder<C8YJwtRetriever, Sequential> {
        let server = C8YJwtRetriever {
            mqtt_config: mqtt_config.with_subscriptions(TopicFilter::new_unchecked("c8y/s/dat")),
        };
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
        let mut mqtt_con = Connection::new(&self.mqtt_config)
            .await
            .map_err(|_| SmartRestDeserializerError::NoResponse)?;

        // Ignore errors on this connection
        mqtt_con.errors.close();

        mqtt_con
            .published
            .publish(mqtt_channel::Message::new(
                &Topic::new_unchecked("c8y/s/uat"),
                "".to_string(),
            ))
            .await
            .map_err(|_| SmartRestDeserializerError::NoResponse)?;

        let token_smartrest =
            match tokio::time::timeout(Duration::from_secs(10), mqtt_con.received.next()).await {
                Ok(Some(msg)) => msg.payload_str().unwrap_or("non-utf8").to_string(),
                _ => return Err(SmartRestDeserializerError::NoResponse),
            };

        let token = SmartRestJwtResponse::try_new(&token_smartrest)?;
        Ok(token.token())
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
