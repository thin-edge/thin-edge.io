use async_trait::async_trait;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_channel::Connection;
use mqtt_channel::PubChannel;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use std::convert::Infallible;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::ConnectionBuilder;
use tedge_actors::DynSender;
use tedge_actors::RequestResponseHandler;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::Service;
use tedge_actors::ServiceActor;
use tedge_actors::ServiceMessageBoxBuilder;

pub type JwtRequest = ();
pub type JwtResult = Result<Option<String>, SmartRestDeserializerError>;

/// Retrieves JWT tokens authenticating the device
pub type JwtRetriever = RequestResponseHandler<JwtRequest, JwtResult>;

/// A JwtRetriever that gets JWT tokens from C8Y over MQTT
pub struct C8YJwtRetriever {
    mqtt_config: mqtt_channel::Config,
}

impl C8YJwtRetriever {
    pub fn builder(mqtt_config: mqtt_channel::Config) -> JwtRetrieverBuilder<C8YJwtRetriever> {
        JwtRetrieverBuilder::new(C8YJwtRetriever {
            mqtt_config: mqtt_config.with_subscriptions(TopicFilter::new_unchecked("c8y/s/dat")),
        })
    }
}

#[async_trait]
impl Service for C8YJwtRetriever {
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
        Ok(Some(token.token()))
    }
}

/// A JwtRetriever that simply always returns the same JWT token (possibly none)
pub struct ConstJwtRetriever {
    token: Option<String>,
}

#[async_trait]
impl Service for ConstJwtRetriever {
    type Request = JwtRequest;
    type Response = JwtResult;

    fn name(&self) -> &str {
        "ConstJwtRetriever"
    }

    async fn handle(&mut self, _request: Self::Request) -> Self::Response {
        Ok(self.token.clone())
    }
}

/// Build an actor from a JwtRetriever service
pub struct JwtRetrieverBuilder<S: Service<Request = JwtRequest, Response = JwtResult>> {
    actor: ServiceActor<S>,
    message_box: ServiceMessageBoxBuilder<(), JwtResult>,
}

impl<S: Service<Request = JwtRequest, Response = JwtResult>> JwtRetrieverBuilder<S> {
    pub fn new(service: S) -> Self {
        let actor = ServiceActor::new(service);
        let message_box = ServiceMessageBoxBuilder::new(actor.name(), 10);
        JwtRetrieverBuilder { actor, message_box }
    }
}

#[async_trait]
impl<S: Service<Request = JwtRequest, Response = JwtResult>> ActorBuilder
    for JwtRetrieverBuilder<S>
{
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        runtime.run(self.actor, self.message_box.build()).await
    }
}

impl<S: Service<Request = JwtRequest, Response = JwtResult>>
    ConnectionBuilder<(), JwtResult, (), Infallible> for JwtRetrieverBuilder<S>
{
    fn connect(
        &mut self,
        _config: (),
        output_sender: DynSender<JwtResult>,
    ) -> Result<DynSender<()>, Infallible> {
        Ok(self.message_box.connect(output_sender))
    }
}
