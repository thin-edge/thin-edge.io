use crate::software_manager::actor::SoftwareManagerActor;
use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::software_manager::config::SoftwareManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;

pub struct SoftwareManagerBuilder {
    config: SoftwareManagerConfig,
    message_box: SimpleMessageBoxBuilder<SoftwareRequest, SoftwareResponse>,
}

impl SoftwareManagerBuilder {
    pub fn new(config: SoftwareManagerConfig) -> Self {
        let message_box = SimpleMessageBoxBuilder::new("SoftwareManager", 10);

        Self {
            config,
            message_box,
        }
    }
}

impl ServiceProvider<SoftwareRequest, SoftwareResponse, NoConfig> for SoftwareManagerBuilder {
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<SoftwareResponse>,
    ) -> DynSender<SoftwareRequest> {
        self.message_box.connect_consumer(config, response_sender)
    }
}

impl RuntimeRequestSink for SoftwareManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<SoftwareManagerActor> for SoftwareManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<SoftwareManagerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> SoftwareManagerActor {
        SoftwareManagerActor::new(self.config, self.message_box.build())
    }
}
