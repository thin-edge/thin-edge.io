mod actor;
mod messages;

#[cfg(test)]
mod tests;

#[cfg(feature = "test_helpers")]
pub mod test_helpers;

pub use messages::*;
use std::convert::Infallible;

use actor::*;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::ConcurrentServerActor;
use tedge_actors::ConcurrentServerMessageBox;
use tedge_actors::DynSender;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServerMessageBoxBuilder;
use tedge_actors::ServiceConsumer;
use tedge_actors::ServiceProvider;

pub type HttpHandle = ClientMessageBox<HttpRequest, HttpResult>;
pub trait HttpConnectionBuilder: ServiceProvider<HttpRequest, HttpResult, NoConfig> {}
impl<T> HttpConnectionBuilder for T where T: ServiceProvider<HttpRequest, HttpResult, NoConfig> {}

pub struct HttpActorBuilder {
    actor: ConcurrentServerActor<HttpService>,
    pub box_builder: ServerMessageBoxBuilder<HttpRequest, HttpResult>,
}

impl HttpActorBuilder {
    pub fn new() -> Result<Self, HttpError> {
        let service = HttpService::new()?;
        let actor = ConcurrentServerActor::new(service);
        let box_builder = ServerMessageBoxBuilder::new("HTTP", 16).with_max_concurrency(4);

        Ok(HttpActorBuilder { actor, box_builder })
    }

    pub async fn run(self) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let messages = self.box_builder.build();

        actor.run(messages).await
    }
}

impl
    Builder<(
        ConcurrentServerActor<HttpService>,
        ConcurrentServerMessageBox<HttpRequest, HttpResult>,
    )> for HttpActorBuilder
{
    type Error = Infallible;

    fn try_build(
        self,
    ) -> Result<
        (
            ConcurrentServerActor<HttpService>,
            ConcurrentServerMessageBox<HttpRequest, HttpResult>,
        ),
        Self::Error,
    > {
        Ok(self.build())
    }

    fn build(
        self,
    ) -> (
        ConcurrentServerActor<HttpService>,
        ConcurrentServerMessageBox<HttpRequest, HttpResult>,
    ) {
        let actor = self.actor;
        let actor_box = self.box_builder.build();
        (actor, actor_box)
    }
}

impl ServiceProvider<HttpRequest, HttpResult, NoConfig> for HttpActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl ServiceConsumer<HttpRequest, HttpResult>,
        config: NoConfig,
    ) {
        self.box_builder.connect_with(peer, config)
    }
}

impl RuntimeRequestSink for HttpActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}
