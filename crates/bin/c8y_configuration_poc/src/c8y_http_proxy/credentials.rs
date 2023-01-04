use async_trait::async_trait;
use mqtt_channel::StreamExt;
use std::convert::Infallible;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ConnectionBuilder;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::RequestResponseHandler;
use tedge_actors::SimpleMessageBox;
use tedge_http_ext::HttpConnectionBuilder;
use tedge_http_ext::HttpHandle;

/// Retrieves JWT tokens authenticating the device
pub type JwtRetriever = RequestResponseHandler<(), Option<String>>;
pub trait JwtRetrieverBuilder: ConnectionBuilder<(), Option<String>, (), Infallible> {}

/// A JwtRetriever that gets JWT tokens from C8Y over MQTT
pub struct C8YJwtRetriever {}

impl C8YJwtRetriever {
    pub fn new_handle() -> JwtRetriever {
        todo!();
    }
}

#[async_trait]
impl Actor for C8YJwtRetriever {
    type MessageBox = SimpleMessageBox<(), Option<String>>;

    async fn run(
        mut self,
        mut messages: SimpleMessageBox<(), Option<String>>,
    ) -> Result<(), ChannelError> {
        while let Some(message) = messages.recv().await {
            messages.send(None).await?
        }
        Ok(())
    }
}

/// A JwtRetriever that simply always returns no JWT token
pub struct NoJwtRetriever;

impl NoJwtRetriever {
    pub fn new_handle() -> JwtRetriever {
        todo!();
    }
}

#[async_trait]
impl Actor for NoJwtRetriever {
    type MessageBox = SimpleMessageBox<(), Option<String>>;

    async fn run(
        mut self,
        mut messages: SimpleMessageBox<(), Option<String>>,
    ) -> Result<(), ChannelError> {
        while let Some(message) = messages.recv().await {
            messages.send(None).await?
        }
        Ok(())
    }
}
