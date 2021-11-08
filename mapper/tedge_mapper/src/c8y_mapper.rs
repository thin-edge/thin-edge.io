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
        let mut topic_fiter = make_valid_topic_filter_or_panic("tedge/measurements");
        let () = topic_fiter.add("tedge/measurements/+")?;

        let mapper_config = MapperConfig {
            in_topic_filter: topic_fiter,
            out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
            errors_topic: make_valid_topic_or_panic("tedge/errors"),
        };

        let size_threshold = SizeThreshold(16 * 1024);

        let converter = Box::new(CumulocityConverter { size_threshold });

        let mapper = create_mapper(
            CUMULOCITY_MAPPER_NAME,
            &tedge_config,
            mapper_config,
            converter,
        )
        .await?;

        mapper
            .run()
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
