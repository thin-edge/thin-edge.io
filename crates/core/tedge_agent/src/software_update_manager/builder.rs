use crate::software_update_manager::actor::SoftwareUpdateManagerActor;
use crate::software_update_manager::actor::SoftwareUpdateManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;

pub struct SoftwareUpdateManagerBuilder {
    config: SoftwareUpdateManagerConfig,
    message_box: SimpleMessageBoxBuilder<SoftwareUpdateRequest, SoftwareUpdateResponse>,
}

impl SoftwareUpdateManagerBuilder {
    pub fn new(config: SoftwareUpdateManagerConfig) -> Self {
        let message_box = SimpleMessageBoxBuilder::new("SoftwareUpdateManager", 10);

        Self {
            config,
            message_box,
        }
    }
}

impl ServiceProvider<SoftwareUpdateRequest, SoftwareUpdateResponse, NoConfig>
    for SoftwareUpdateManagerBuilder
{
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<SoftwareUpdateResponse>,
    ) -> DynSender<SoftwareUpdateRequest> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl RuntimeRequestSink for SoftwareUpdateManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<SoftwareUpdateManagerActor> for SoftwareUpdateManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<SoftwareUpdateManagerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> SoftwareUpdateManagerActor {
        SoftwareUpdateManagerActor::new(self.config, self.message_box.build())
    }
}
