mod actor;
mod messages;

#[cfg(test)]
mod tests;

pub use messages::*;
use std::convert::Infallible;

use actor::*;
use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::ActorBuilder;
use tedge_actors::ChannelError;
use tedge_actors::ConnectionBuilder;
use tedge_actors::DynSender;
use tedge_actors::RequestResponseHandler;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_actors::ServiceMessageBoxBuilder;

pub type HttpHandle = RequestResponseHandler<HttpRequest, HttpResult>;
pub trait HttpConnectionBuilder:
    ConnectionBuilder<HttpRequest, HttpResult, (), Infallible>
{
}

pub struct HttpActorBuilder {
    actor: HttpActor,
    pub box_builder: ServiceMessageBoxBuilder<HttpRequest, HttpResult>,
}

impl HttpActorBuilder {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let actor = HttpActor::new(config)?;
        let box_builder = ServiceMessageBoxBuilder::new("HTTP", 16);

        Ok(HttpActorBuilder { actor, box_builder })
    }

    pub async fn run(self) -> Result<(), ChannelError> {
        let max_concurrency = 4;
        let actor = self.actor;
        let messages = self.box_builder.build_concurrent(max_concurrency);

        actor.run(messages).await
    }
}

#[async_trait]
impl ActorBuilder for HttpActorBuilder {
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let messages = self.box_builder.build_concurrent(4);
        runtime.run(actor, messages).await
    }
}

impl ConnectionBuilder<HttpRequest, HttpResult, (), Infallible> for HttpActorBuilder {
    fn connect(
        &mut self,
        _config: (),
        client: DynSender<HttpResult>,
    ) -> Result<DynSender<HttpRequest>, Infallible> {
        Ok(self.box_builder.connect(client))
    }
}

impl HttpConnectionBuilder for HttpActorBuilder {}
