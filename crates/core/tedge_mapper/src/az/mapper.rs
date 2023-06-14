use crate::core::component::TEdgeComponent;
use crate::core::mapper::start_basic_actors;
use async_trait::async_trait;
use az_mapper_ext::converter::AzureConverter;
use clock::WallClock;
use std::path::Path;
use tedge_actors::ConvertingActor;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_config::new::TEdgeConfig;

const AZURE_MAPPER_NAME: &str = "tedge-mapper-az";

pub struct AzureMapper {}

impl AzureMapper {
    pub fn new() -> AzureMapper {
        AzureMapper {}
    }
}

#[async_trait]
impl TEdgeComponent for AzureMapper {
    fn session_name(&self) -> &str {
        AZURE_MAPPER_NAME
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors(self.session_name(), &tedge_config).await?;

        let az_converter =
            AzureConverter::new(tedge_config.az.mapper.timestamp, Box::new(WallClock));
        let mut az_converting_actor = ConvertingActor::builder(
            "AzConverter",
            az_converter,
            AzureConverter::in_topic_filter(),
        );
        az_converting_actor.add_input(&mut mqtt_actor);

        az_converting_actor.register_peer(NoConfig, mqtt_actor.get_sender());

        runtime.spawn(az_converting_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
