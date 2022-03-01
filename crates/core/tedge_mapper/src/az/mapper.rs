use crate::{
    az::converter::AzureConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use async_trait::async_trait;
use clock::WallClock;
use tedge_config::{AzureMapperTimestamp, MqttBindAddressSetting, TEdgeConfig};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting};
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
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();
        let clock = Box::new(WallClock);
        let size_threshold = SizeThreshold(255 * 1024);

        let converter = Box::new(AzureConverter::new(add_timestamp, clock, size_threshold));

        let mut mapper = create_mapper(AZURE_MAPPER_NAME, mqtt_host, mqtt_port, converter).await?;

        mapper
            .run()
            .instrument(info_span!(AZURE_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
