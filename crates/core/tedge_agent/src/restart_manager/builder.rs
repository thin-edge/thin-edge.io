use crate::restart_manager::actor::RestartManagerActor;
use crate::restart_manager::config::RestartManagerConfig;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::RestartCommand;

pub struct RestartManagerBuilder {
    config: RestartManagerConfig,
    message_box: SimpleMessageBoxBuilder<RestartCommand, RestartCommand>,
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

impl MessageSink<RestartCommand> for RestartManagerBuilder {
    fn get_sender(&self) -> DynSender<RestartCommand> {
        self.message_box.get_sender()
    }
}

impl MessageSource<RestartCommand, NoConfig> for RestartManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<RestartCommand>) {
        self.message_box.connect_sink(config, peer)
    }
}

impl MessageSource<GenericCommandData, NoConfig> for RestartManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.message_box.connect_sink(config, &peer.get_sender())
    }
}

impl IntoIterator for &RestartManagerBuilder {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let sender =
            MappingSender::new(self.message_box.get_sender(), |msg: GenericCommandState| {
                msg.try_into().ok()
            });
        vec![(OperationType::Restart.to_string(), sender.into())].into_iter()
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
