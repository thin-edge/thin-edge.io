mod actor;

use crate::actor::GenMapper;
use std::convert::Infallible;
use std::path::Path;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct GenMapperBuilder {
    message_box: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl GenMapperBuilder {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        let _config_dir = config_dir.as_ref();
        let messages = SimpleMessageBoxBuilder::new("WasmMapper", 16);
        GenMapperBuilder {
            message_box: messages,
        }
    }

    pub fn connect(
        &mut self,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) {
        mqtt.connect_sink(self.topics(), &self.message_box);
        self.message_box.connect_sink(NoConfig, mqtt);
    }

    fn topics(&self) -> TopicFilter {
        TopicFilter::empty()
    }
}

impl RuntimeRequestSink for GenMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.message_box.get_signal_sender()
    }
}

impl Builder<GenMapper> for GenMapperBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<GenMapper, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> GenMapper {
        GenMapper::new(self.message_box.build())
    }
}
