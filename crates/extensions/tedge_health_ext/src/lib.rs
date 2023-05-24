mod actor;

#[cfg(test)]
mod tests;

use crate::actor::HealthInput;
use actor::HealthMonitorActor;
use tedge_actors::adapt;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageSink;
use tedge_actors::NoConfig;
use tedge_actors::NullSender;
use tedge_actors::RuntimeAction;
use tedge_actors::RuntimeEvent;
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
    box_builder: SimpleMessageBoxBuilder<HealthInput, MqttMessage>,
    runtime: DynSender<RuntimeAction>,
}

impl HealthMonitorBuilder {
    pub fn new(service_name: &str) -> Self {
        let box_builder: SimpleMessageBoxBuilder<HealthInput, MqttMessage> =
            SimpleMessageBoxBuilder::new(service_name, 64);
        let runtime = NullSender.into();
        HealthMonitorBuilder {
            service_name: service_name.to_owned(),
            box_builder,
            runtime,
        }
    }

    pub fn with_mqtt(
        service_name: &str,
        mqtt: &mut (impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> + AsMut<MqttConfig>),
    ) -> Self {
        let mut builder = HealthMonitorBuilder::new(service_name);
        builder.connect_to_mqtt(mqtt);
        builder
    }

    pub fn connect_to_mqtt(
        &mut self,
        mqtt: &mut (impl ServiceProvider<MqttMessage, MqttMessage, TopicFilter> + AsMut<MqttConfig>),
    ) {
        let subscriptions = vec![
            "tedge/health-check",
            &format!("tedge/health-check/{0}", self.service_name),
        ]
        .try_into()
        .expect("Failed to create the HealthMonitorActor topic filter");

        self.box_builder.set_request_sender(
            mqtt.connect_consumer(subscriptions, adapt(&self.box_builder.get_sender())),
        );

        // Update the MQTT config
        *mqtt.as_mut() = self.set_init_and_last_will(mqtt.as_mut().clone());
    }

    fn set_init_and_last_will(&self, config: MqttConfig) -> MqttConfig {
        let name = self.service_name.to_owned();
        config
            .with_initial_message(move || health_status_up_message(&name))
            .with_last_will_message(health_status_down_message(&self.service_name))
    }
}

impl ServiceProvider<RuntimeEvent, RuntimeAction, NoConfig> for HealthMonitorBuilder {
    fn connect_consumer(
        &mut self,
        _config: NoConfig,
        response_sender: DynSender<RuntimeAction>,
    ) -> DynSender<RuntimeEvent> {
        self.runtime = response_sender;
        adapt(&self.box_builder.get_sender())
    }
}

impl RuntimeRequestSink for HealthMonitorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.box_builder.get_signal_sender())
    }

    fn set_event_sender(&mut self, event_sender: DynSender<RuntimeEvent>) {
        self.box_builder.set_event_sender(event_sender)
    }
}

impl Builder<HealthMonitorActor> for HealthMonitorBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<HealthMonitorActor, Self::Error> {
        let message_box = self.box_builder.build();
        let actor = HealthMonitorActor::new(self.service_name, message_box, self.runtime);

        Ok(actor)
    }
}
