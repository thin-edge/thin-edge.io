use crate::c8y_converter::CumulocityConverter;
use crate::component::TEdgeComponent;
use crate::mapper::*;
use async_trait::async_trait;
use tedge_config::TEdgeConfig;
use tracing::{debug_span, Instrument};

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";

pub struct CumulocityMapper {}

impl CumulocityMapper {
    pub fn new() -> CumulocityMapper {
        CumulocityMapper {}
    }
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mapper_config = MapperConfig {
            in_topic: make_valid_topic_or_panic("tedge/measurements"),
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let converter = Box::new(CumulocityConverter);

        let mapper = create_mapper(
            CUMULOCITY_MAPPER_NAME,
            &tedge_config,
            mapper_config,
            converter,
        )
        .await?;

        mapper
            .run()
            .instrument(debug_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
