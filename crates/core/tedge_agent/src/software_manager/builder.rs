use crate::software_manager::actor::SoftwareCommand;
use crate::software_manager::actor::SoftwareManagerActor;
use crate::software_manager::config::SoftwareManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;

pub struct SoftwareManagerBuilder {
    config: SoftwareManagerConfig,
    message_box: SimpleMessageBoxBuilder<SoftwareCommand, SoftwareCommand>,
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

impl MessageSink<SoftwareCommand> for SoftwareManagerBuilder {
    fn get_sender(&self) -> DynSender<SoftwareCommand> {
        self.message_box.get_sender()
    }
}

impl MessageSource<SoftwareCommand, NoConfig> for SoftwareManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<SoftwareCommand>) {
        self.message_box.connect_sink(config, peer)
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
