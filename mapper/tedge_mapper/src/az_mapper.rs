use crate::az_converter::AzureConverter;
use crate::component::TEdgeComponent;
use crate::mapper::*;
use crate::size_threshold::SizeThreshold;
use async_trait::async_trait;
use clock::WallClock;
use tedge_config::ConfigSettingAccessor;
use tedge_config::{AzureMapperTimestamp, TEdgeConfig};
use tracing::{info_span, Instrument};

const AZURE_MAPPER_NAME: &str = "tedge-mapper-az";

pub struct AzureMapper {}

impl AzureMapper {
    pub fn new() -> AzureMapper {
        AzureMapper {}
    }
}

#[async_trait]
impl TEdgeComponent for AzureMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let add_timestamp = tedge_config.query(AzureMapperTimestamp)?.is_set();
        let clock = Box::new(WallClock);
        let size_threshold = SizeThreshold(255 * 1024);

        let converter = Box::new(AzureConverter::new(
            add_timestamp,
            clock,
            size_threshold,
        ));

        let mut mapper =
            create_mapper(AZURE_MAPPER_NAME, &tedge_config, converter).await?;

        mapper
            .run()
            .instrument(info_span!(AZURE_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
