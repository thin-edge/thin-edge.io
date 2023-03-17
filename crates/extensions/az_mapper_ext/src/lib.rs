mod actor;
mod converter;
pub mod mapper;

use actor::AzMapperActor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

type AzureMapperMessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;

pub struct AzureMapperBuilder {
    subscriptions: TopicFilter,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    add_time_stamp: bool,
}

impl AzureMapperBuilder {
    pub fn new(service_name: &str, add_time_stamp: bool) -> Self {
        let subscriptions = vec!["tedge/measurements", "tedge/measurements/+"]
            .try_into()
            .expect("Failed to create the AzureMapperActor topicfilter");
        AzureMapperBuilder {
            subscriptions,
            box_builder: SimpleMessageBoxBuilder::new(service_name, 16),
            add_time_stamp,
        }
    }

    pub fn add_input(&mut self, source: &mut impl MessageSource<MqttMessage, TopicFilter>) {
        source.register_peer(self.subscriptions.clone(), self.box_builder.get_sender())
    }
}

impl RuntimeRequestSink for AzureMapperBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.box_builder.get_signal_sender())
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for AzureMapperBuilder {
    fn get_config(&self) -> TopicFilter {
        self.subscriptions.clone()
    }

    fn set_request_sender(&mut self, request_sender: DynSender<MqttMessage>) {
        self.box_builder.set_request_sender(request_sender);
    }

    fn get_response_sender(&self) -> DynSender<MqttMessage> {
        self.box_builder.get_sender()
    }
}

impl Builder<(AzMapperActor, AzureMapperMessageBox)> for AzureMapperBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<(AzMapperActor, AzureMapperMessageBox), tedge_actors::LinkError> {
        let message_box = self.box_builder.build();

        let actor = AzMapperActor::new(self.add_time_stamp);

        Ok((actor, message_box))
    }
}
