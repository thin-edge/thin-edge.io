mod actor;

#[cfg(test)]
mod tests;

use actor::HealthMonitorActor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

type HealthMonitorMessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;

pub struct HealthMonitorBuilder {
    service_name: String,
    subscriptions: TopicFilter,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl HealthMonitorBuilder {
    pub fn new(service_name: &str) -> Self {
        let subscriptions = vec![
            "tedge/health-check",
            &format!("tedge/health-check/{service_name}"),
        ]
        .try_into()
        .expect("Failed to create the HealthMonitorActor topicfilter");
        HealthMonitorBuilder {
            service_name: service_name.to_owned(),
            subscriptions,
            box_builder: SimpleMessageBoxBuilder::new(service_name, 16),
        }
    }

    pub fn set_init_and_last_will(&self, config: MqttConfig) -> MqttConfig {
        let name = self.service_name.to_owned();
        config
            .with_initial_message(move || health_status_up_message(&name))
            .with_last_will_message(health_status_down_message(&self.service_name))
    }
}

impl RuntimeRequestSink for HealthMonitorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.box_builder.get_signal_sender())
    }
}

impl Builder<(HealthMonitorActor, HealthMonitorMessageBox)> for HealthMonitorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<(HealthMonitorActor, HealthMonitorMessageBox), Self::Error> {
        let message_box = self.box_builder.build();

        let actor = HealthMonitorActor::new(self.service_name);

        Ok((actor, message_box))
    }
}

impl ServiceConsumer<MqttMessage, MqttMessage, TopicFilter> for HealthMonitorBuilder {
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
