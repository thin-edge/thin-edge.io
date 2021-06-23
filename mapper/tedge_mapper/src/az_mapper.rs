use crate::az_converter::AzureConverter;
use crate::component::TEdgeComponent;
use crate::mapper::*;
use crate::size_threshold::SizeThreshold;
use async_trait::async_trait;
use clock::WallClock;
use tedge_config::ConfigSettingAccessor;
use tedge_config::{AzureMapperTimestamp, TEdgeConfig};
use tracing::{debug_span, Instrument};

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
        let mapper_config = MapperConfig {
            in_topic: make_valid_topic_or_panic("tedge/measurements"),
            out_topic: make_valid_topic_or_panic("az/messages/events"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let add_timestamp = tedge_config.query(AzureMapperTimestamp)?.is_set();
        let clock = Box::new(WallClock);
        let size_threshold = SizeThreshold(255 * 1024);

        let converter = Box::new(AzureConverter {
            add_timestamp,
            clock,
            size_threshold,
        });

        let mapper =
            create_mapper(AZURE_MAPPER_NAME, &tedge_config, mapper_config, converter).await?;

        mapper
            .run()
            .instrument(debug_span!(AZURE_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
