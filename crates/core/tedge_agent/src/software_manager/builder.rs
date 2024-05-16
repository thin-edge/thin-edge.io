use crate::software_manager::actor::SoftwareCommand;
use crate::software_manager::actor::SoftwareManagerActor;
use crate::software_manager::config::SoftwareManagerConfig;
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
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareUpdateCommand;

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

impl MessageSource<GenericCommandData, NoConfig> for SoftwareManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.message_box
            .connect_mapped_sink(config, &peer.get_sender(), |msg: SoftwareCommand| {
                msg.into_generic_commands()
            })
    }
}

impl IntoIterator for &SoftwareManagerBuilder {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let software_list_sender =
            MappingSender::new(self.message_box.get_sender(), |msg: GenericCommandState| {
                SoftwareListCommand::try_from(msg)
                    .map(SoftwareCommand::SoftwareListCommand)
                    .ok()
            });
        let software_update_sender =
            MappingSender::new(self.message_box.get_sender(), |msg: GenericCommandState| {
                SoftwareUpdateCommand::try_from(msg)
                    .map(SoftwareCommand::SoftwareUpdateCommand)
                    .ok()
            })
            .into();
        vec![
            (
                OperationType::SoftwareList.to_string(),
                software_list_sender.into(),
            ),
            (
                OperationType::SoftwareUpdate.to_string(),
                software_update_sender,
            ),
        ]
        .into_iter()
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
