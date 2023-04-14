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
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct HealthMonitorBuilder {
    service_name: String,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl HealthMonitorBuilder {
    pub fn new(
        service_name: &str,
        mqtt: &mut (impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> + AsMut<MqttConfig>),
    ) -> Self {
        // Connect this actor to MQTT
        let subscriptions = vec![
            "tedge/health-check",
            &format!("tedge/health-check/{service_name}"),
        ]
        .try_into()
        .expect("Failed to create the HealthMonitorActor topic filter");

        let mut box_builder = SimpleMessageBoxBuilder::new(service_name, 16);
        box_builder
            .set_request_sender(mqtt.connect_consumer(subscriptions, box_builder.get_sender()));

        let builder = HealthMonitorBuilder {
            service_name: service_name.to_owned(),
            box_builder,
        };

        // Update the MQTT config
        *mqtt.as_mut() = builder.set_init_and_last_will(mqtt.as_mut().clone());

        builder
    }

    fn set_init_and_last_will(&self, config: MqttConfig) -> MqttConfig {
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

impl Builder<HealthMonitorActor> for HealthMonitorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<HealthMonitorActor, Self::Error> {
        let message_box = self.box_builder.build();
        let actor = HealthMonitorActor::new(self.service_name, message_box);

        Ok(actor)
    }
}
