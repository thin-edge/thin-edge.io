use crate::c8y_converter::CumulocityConverter;
use crate::component::TEdgeComponent;
use crate::mapper::*;
use crate::size_threshold::SizeThreshold;
use async_trait::async_trait;
use tedge_config::TEdgeConfig;
use tracing::{info_span, Instrument};

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
        let size_threshold = SizeThreshold(16 * 1024);

        let converter = Box::new(CumulocityConverter::new(size_threshold));

        let mut mapper = create_mapper(CUMULOCITY_MAPPER_NAME, &tedge_config, converter).await?;

        mapper
            .run("c8y")
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
