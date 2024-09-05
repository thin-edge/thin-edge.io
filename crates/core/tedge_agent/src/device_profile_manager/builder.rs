use super::actor::DeviceProfileManagerActor;
use camino::Utf8PathBuf;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::LoggingSender;
use tedge_actors::MappingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::device_profile::DeviceProfileCmd;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::FileError;

pub struct DeviceProfileManagerBuilder {
    message_box: SimpleMessageBoxBuilder<DeviceProfileCmd, DeviceProfileCmd>,
    mqtt_schema: MqttSchema,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

impl DeviceProfileManagerBuilder {
    pub fn try_new(
        mqtt_schema: MqttSchema,
        ops_dir: &Utf8PathBuf,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) -> Result<Self, FileError> {
        let workflow_file = ops_dir.join("device_profile.toml");
        if !workflow_file.exists() {
            let workflow_definition = include_str!("../resources/device_profile.toml");

            create_file_with_defaults(workflow_file, Some(workflow_definition))?;
        }
        let message_box = SimpleMessageBoxBuilder::new("DeviceProfileManager", 10);

        let mqtt_publisher = LoggingSender::new("MqttPublisher".into(), mqtt_actor.get_sender());
        Ok(Self {
            message_box,
            mqtt_schema,
            mqtt_publisher,
        })
    }
}

impl MessageSink<DeviceProfileCmd> for DeviceProfileManagerBuilder {
    fn get_sender(&self) -> DynSender<DeviceProfileCmd> {
        self.message_box.get_sender()
    }
}

impl MessageSource<GenericCommandData, NoConfig> for DeviceProfileManagerBuilder {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.message_box.connect_sink(config, &peer.get_sender())
    }
}

impl IntoIterator for &DeviceProfileManagerBuilder {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let sender =
            MappingSender::new(self.message_box.get_sender(), |msg: GenericCommandState| {
                msg.try_into().ok()
            });
        vec![(OperationType::DeviceProfile.to_string(), sender.into())].into_iter()
    }
}

impl RuntimeRequestSink for DeviceProfileManagerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<DeviceProfileManagerActor> for DeviceProfileManagerBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<DeviceProfileManagerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> DeviceProfileManagerActor {
        DeviceProfileManagerActor::new(
            self.message_box.build(),
            self.mqtt_schema,
            self.mqtt_publisher,
        )
    }
}
