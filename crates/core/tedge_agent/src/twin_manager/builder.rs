use crate::twin_manager::actor::TwinManagerActor;
use camino::Utf8PathBuf;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;

pub struct TwinManagerConfig {
    pub config_dir: Utf8PathBuf,
    pub mqtt_schema: MqttSchema,
    pub device_topic_id: EntityTopicId,
    pub agent_topic_id: EntityTopicId,
}

impl TwinManagerConfig {
    pub fn new(
        config_dir: Utf8PathBuf,
        mqtt_schema: MqttSchema,
        device_topic_id: EntityTopicId,
        agent_topic_id: EntityTopicId,
    ) -> Self {
        Self {
            config_dir,
            mqtt_schema,
            device_topic_id,
            agent_topic_id,
        }
    }

    pub fn subscriptions(&self) -> TopicFilter {
        let mut topics = self.mqtt_schema.topics(
            EntityFilter::Entity(&self.device_topic_id),
            ChannelFilter::EntityTwinData,
        );

        // Subscribe to the agent health status just to ensure that at least one message is received by this actor,
        // which will trigger the inventory data processing.
        topics.add_all(self.mqtt_schema.topics(
            EntityFilter::Entity(&self.agent_topic_id),
            ChannelFilter::Health,
        ));

        topics
    }
}

pub struct TwinManagerActorBuilder {
    config: TwinManagerConfig,
    box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

impl TwinManagerActorBuilder {
    pub fn new(
        config: TwinManagerConfig,
        mqtt_actor: &mut (impl MessageSource<MqttMessage, TopicFilter> + MessageSink<MqttMessage>),
    ) -> Self {
        let mqtt_publisher = LoggingSender::new(
            "TwinManagerToMqttPublisher".into(),
            mqtt_actor.get_sender().get_sender(),
        );
        let messages = SimpleMessageBoxBuilder::new("TwinManagerActor", 64);
        let subscriptions = config.subscriptions();
        let actor_builder = Self {
            config,
            box_builder: messages,
            mqtt_publisher,
        };
        mqtt_actor.connect_sink(subscriptions, &actor_builder);

        actor_builder
    }
}

impl MessageSink<MqttMessage> for TwinManagerActorBuilder {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.box_builder.get_sender()
    }
}

impl RuntimeRequestSink for TwinManagerActorBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<TwinManagerActor> for TwinManagerActorBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<TwinManagerActor, Self::Error> {
        Ok(TwinManagerActor::new(
            self.config,
            self.box_builder.build(),
            self.mqtt_publisher,
        ))
    }
}
