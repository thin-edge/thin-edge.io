use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use batcher::BatchingActorBuilder;
use collectd_ext::actor::CollectdActorBuilder;
use mqtt_channel::QoS;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_actors::MessageSink;
use tedge_actors::NoConfig;
use tedge_config::TEdgeConfig;

const COLLECTD_MAPPER_NAME: &str = "tedge-mapper-collectd";
const COLLECTD_INPUT_TOPICS: &str = "collectd/#";
const COLLECTD_OUTPUT_TOPIC: &str = "te/device/main///m/";

pub struct CollectdMapper;

impl CollectdMapper {
    fn input_topics() -> TopicFilter {
        TopicFilter::new_unchecked(COLLECTD_INPUT_TOPICS).with_qos(QoS::AtMostOnce)
    }

    fn output_topic() -> Topic {
        Topic::new_unchecked(COLLECTD_OUTPUT_TOPIC)
    }
}

#[async_trait]
impl TEdgeComponent for CollectdMapper {
    fn session_name(&self) -> &str {
        COLLECTD_MAPPER_NAME
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;

        let input_topic = CollectdMapper::input_topics();
        let output_topic = CollectdMapper::output_topic();

        let mut batching_actor = BatchingActorBuilder::default();
        let mut collectd_actor = CollectdActorBuilder::new(input_topic);

        collectd_actor.add_input(&mut mqtt_actor);
        batching_actor.connect_source(NoConfig, &mut collectd_actor);
        mqtt_actor.connect_mapped_source(NoConfig, &mut batching_actor, move |batch| {
            collectd_ext::converter::batch_into_mqtt_messages(&output_topic, batch).into_iter()
        });

        runtime.spawn(collectd_actor).await?;
        runtime.spawn(batching_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
