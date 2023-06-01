use std::path::Path;

use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use aws_mapper_ext::converter::AwsConverter;

use async_trait::async_trait;
use clock::WallClock;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_config::AwsMapperTimestamp;
use tedge_config::ConfigSettingAccessor;
use tedge_config::TEdgeConfig;

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
        let aws_converter =
            AwsConverter::new(tedge_config.query(AwsMapperTimestamp)?.is_set(), clock);
        let mut aws_converting_actor = ConvertingActor::builder(
            "AwsConverter",
            aws_converter,
            AwsConverter::in_topic_filter(),
        );

        aws_converting_actor.add_input(&mut mqtt_actor);
        aws_converting_actor.register_peer(NoConfig, mqtt_actor.get_sender());

        runtime.spawn(aws_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
