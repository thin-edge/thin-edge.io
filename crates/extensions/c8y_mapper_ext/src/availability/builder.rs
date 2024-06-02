use crate::availability::actor::AvailabilityActor;
use crate::availability::AvailabilityConfig;
use crate::availability::AvailabilityInput;
use crate::availability::AvailabilityOutput;
use crate::availability::TimerComplete;
use crate::availability::TimerStart;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct AvailabilityBuilder {
    config: AvailabilityConfig,
    box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput>,
    mqtt_publisher: DynSender<MqttMessage>,
    timer_sender: DynSender<TimerStart>,
}

impl AvailabilityBuilder {
    pub fn new(
        config: AvailabilityConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
        timer: &mut impl Service<TimerStart, TimerComplete>,
    ) -> Self {
        let box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput> =
            SimpleMessageBoxBuilder::new("AvailabilityMonitoring", 16);

        let topics = [
            config
                .mqtt_schema
                .topics(EntityFilter::AnyEntity, ChannelFilter::EntityMetadata),
            config
                .mqtt_schema
                .topics(EntityFilter::AnyEntity, ChannelFilter::Health),
        ]
        .into_iter()
        .collect();
        mqtt.connect_sink(topics, &box_builder.get_sender());
        let mqtt_publisher = mqtt.get_sender();

        let timer_sender = timer.connect_client(box_builder.get_sender().sender_clone());

        Self {
            config,
            box_builder,
            mqtt_publisher,
            timer_sender,
        }
    }
}

impl RuntimeRequestSink for AvailabilityBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<AvailabilityActor> for AvailabilityBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<AvailabilityActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> AvailabilityActor {
        let mqtt_publisher =
            LoggingSender::new("AvailabilityActor => Mqtt".into(), self.mqtt_publisher);
        let timer_sender =
            LoggingSender::new("AvailabilityActor => Timer".into(), self.timer_sender);
        let message_box = self.box_builder.build();

        AvailabilityActor::new(self.config, message_box, mqtt_publisher, timer_sender)
    }
}
