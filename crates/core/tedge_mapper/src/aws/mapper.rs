use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use aws_mapper_ext::converter::AwsConverter;
use clock::WallClock;
use mqtt_channel::TopicFilter;
use std::path::Path;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tracing::warn;

const AWS_MAPPER_NAME: &str = "tedge-mapper-aws";

pub struct AwsMapper;

#[async_trait]
impl TEdgeComponent for AwsMapper {
    fn session_name(&self) -> &str {
        AWS_MAPPER_NAME
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;
        let clock = Box::new(WallClock);
        let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.clone());
        let aws_converter = AwsConverter::new(
            tedge_config.aws.mapper.timestamp,
            clock,
            mqtt_schema,
            tedge_config.aws.mapper.timestamp_format,
        );
        let mut aws_converting_actor = ConvertingActor::builder("AwsConverter", aws_converter);

        aws_converting_actor.connect_source(get_topic_filter(&tedge_config), &mut mqtt_actor);
        aws_converting_actor.connect_sink(NoConfig, &mqtt_actor);

        runtime.spawn(aws_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}

fn get_topic_filter(tedge_config: &TEdgeConfig) -> TopicFilter {
    let mut topics = TopicFilter::empty();
    for topic in tedge_config.aws.topics.0.clone() {
        if topics.add(&topic).is_err() {
            warn!("The configured topic '{topic}' is invalid and ignored.");
        }
    }
    topics
}
