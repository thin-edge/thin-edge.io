use crate::software_list_manager::actor::SoftwareListManagerActor;
use crate::software_list_manager::actor::SoftwareListManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;

pub struct SoftwareListManagerBuilder {
    config: SoftwareListManagerConfig,
    message_box: SimpleMessageBoxBuilder<SoftwareListRequest, SoftwareListResponse>,
}

impl SoftwareListManagerBuilder {
    pub fn new(config: SoftwareListManagerConfig) -> Self {
        let message_box = SimpleMessageBoxBuilder::new("SoftwareListManager", 10);

        Self {
            config,
            message_box,
        }
    }
}

impl ServiceProvider<SoftwareListRequest, SoftwareListResponse, NoConfig>
    for SoftwareListManagerBuilder
{
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<SoftwareListResponse>,
    ) -> DynSender<SoftwareListRequest> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl RuntimeRequestSink for SoftwareListManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<SoftwareListManagerActor> for SoftwareListManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<SoftwareListManagerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> SoftwareListManagerActor {
        SoftwareListManagerActor::new(self.config, self.message_box.build())
    }
}
