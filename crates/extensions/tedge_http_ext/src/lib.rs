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
use tedge_actors::ChannelError;
use tedge_actors::ConcurrentServiceActor;
use tedge_actors::ConcurrentServiceMessageBox;
use tedge_actors::DynSender;
use tedge_actors::MessageBoxPlug;
use tedge_actors::MessageBoxSocket;
use tedge_actors::NoConfig;
use tedge_actors::RequestResponseHandler;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceMessageBoxBuilder;

pub type HttpHandle = RequestResponseHandler<HttpRequest, HttpResult>;
pub trait HttpConnectionBuilder: MessageBoxSocket<HttpRequest, HttpResult, NoConfig> {}
impl<T> HttpConnectionBuilder for T where T: MessageBoxSocket<HttpRequest, HttpResult, NoConfig> {}

pub struct HttpActorBuilder {
    actor: ConcurrentServiceActor<HttpService>,
    pub box_builder: ServiceMessageBoxBuilder<HttpRequest, HttpResult>,
}

impl HttpActorBuilder {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let service = HttpService::new(config)?;
        let actor = ConcurrentServiceActor::new(service);
        let box_builder = ServiceMessageBoxBuilder::new("HTTP", 16).with_max_concurrency(4);

        Ok(HttpActorBuilder { actor, box_builder })
    }

    pub async fn run(self) -> Result<(), ChannelError> {
        let actor = self.actor;
        let messages = self.box_builder.build();

        actor.run(messages).await
    }
}

impl
    Builder<(
        ConcurrentServiceActor<HttpService>,
        ConcurrentServiceMessageBox<HttpRequest, HttpResult>,
    )> for HttpActorBuilder
{
    type Error = Infallible;

    fn try_build(
        self,
    ) -> Result<
        (
            ConcurrentServiceActor<HttpService>,
            ConcurrentServiceMessageBox<HttpRequest, HttpResult>,
        ),
        Self::Error,
    > {
        Ok(self.build())
    }

    fn build(
        self,
    ) -> (
        ConcurrentServiceActor<HttpService>,
        ConcurrentServiceMessageBox<HttpRequest, HttpResult>,
    ) {
        let actor = self.actor;
        let actor_box = self.box_builder.build();
        (actor, actor_box)
    }
}

impl MessageBoxSocket<HttpRequest, HttpResult, NoConfig> for HttpActorBuilder {
    fn connect_with(
        &mut self,
        peer: &mut impl MessageBoxPlug<HttpRequest, HttpResult>,
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
