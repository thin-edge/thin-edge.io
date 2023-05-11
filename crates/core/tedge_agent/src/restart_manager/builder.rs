use crate::restart_manager::actor::RestartManagerActor;
use crate::restart_manager::config::RestartManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::RestartOperationRequest;
use tedge_api::RestartOperationResponse;

pub struct RestartManagerBuilder {
    config: RestartManagerConfig,
    message_box: SimpleMessageBoxBuilder<RestartOperationRequest, RestartOperationResponse>,
}

impl RestartManagerBuilder {
    pub fn new(config: RestartManagerConfig) -> Self {
        let message_box = SimpleMessageBoxBuilder::new("RestartManager", 10);

        Self {
            config,
            message_box,
        }
    }
}

impl ServiceProvider<RestartOperationRequest, RestartOperationResponse, NoConfig>
    for RestartManagerBuilder
{
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<RestartOperationResponse>,
    ) -> DynSender<RestartOperationRequest> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl RuntimeRequestSink for RestartManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<RestartManagerActor> for RestartManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<RestartManagerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> RestartManagerActor {
        RestartManagerActor::new(self.config, self.message_box.build())
    }
}
