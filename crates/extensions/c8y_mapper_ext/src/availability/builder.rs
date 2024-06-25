use crate::actor::PublishMessage;
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
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::HealthStatus;
use tedge_mqtt_ext::MqttMessage;

pub struct AvailabilityBuilder {
    config: AvailabilityConfig,
    box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput>,
    timer_sender: DynSender<TimerStart>,
}

impl AvailabilityBuilder {
    pub fn new(
        config: AvailabilityConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, Vec<ChannelFilter>> + MessageSink<PublishMessage>),
        timer: &mut impl Service<TimerStart, TimerComplete>,
    ) -> Self {
        let mut box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput> =
            SimpleMessageBoxBuilder::new("AvailabilityMonitoring", 16);

        box_builder.connect_mapped_source(
            Self::channels(),
            mqtt,
            Self::mqtt_message_parser(config.clone()),
        );

        mqtt.connect_mapped_source(NoConfig, &mut box_builder, Self::mqtt_message_builder());

        let timer_sender = timer.connect_client(box_builder.get_sender().sender_clone());

        Self {
            config: config.clone(),
            box_builder,
            timer_sender,
        }
    }

    fn channels() -> Vec<ChannelFilter> {
        vec![ChannelFilter::EntityMetadata, ChannelFilter::Health]
    }

    fn mqtt_message_parser(
        config: AvailabilityConfig,
    ) -> impl Fn(MqttMessage) -> Option<AvailabilityInput> {
        move |message| {
            if let Ok((source, channel)) = config.mqtt_schema.entity_channel_of(&message.topic) {
                match channel {
                    Channel::EntityMetadata => {
                        if let Ok(registration_message) =
                            EntityRegistrationMessage::try_from(&message)
                        {
                            return Some(registration_message.into());
                        }
                    }
                    Channel::Health => {
                        let health_status: HealthStatus =
                            serde_json::from_slice(message.payload()).unwrap_or_default();
                        return Some((source, health_status).into());
                    }
                    _ => {}
                }
            }
            None
        }
    }

    fn mqtt_message_builder() -> impl Fn(AvailabilityOutput) -> Option<PublishMessage> {
        move |res| match res {
            AvailabilityOutput::C8ySmartRestSetInterval117(value) => {
                Some(PublishMessage(value.into()))
            }
            AvailabilityOutput::C8yJsonInventoryUpdate(value) => Some(PublishMessage(value.into())),
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
        let timer_sender =
            LoggingSender::new("AvailabilityActor => Timer".into(), self.timer_sender);
        let message_box = self.box_builder.build();

        AvailabilityActor::new(self.config, message_box, timer_sender)
    }
}
