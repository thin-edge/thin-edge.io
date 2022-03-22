use crate::{
    az::converter::AzureConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use async_trait::async_trait;
use clock::WallClock;
use mqtt_channel::TopicFilter;
use tedge_config::{AzureMapperTimestamp, ConfigRepository, MqttBindAddressSetting, TEdgeConfig};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting};
use tedge_utils::file::create_directory_with_user_group;
use tracing::{info, info_span, Instrument};

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

    async fn init(&self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper az");
        create_directory_with_user_group(
            "/etc/tedge/operations/az",
            "tedge-mapper",
            "tedge-mapper",
            0o775,
        )?;

        self.init_session(get_converter()?.mapper_config.in_topic_filter)
            .await?;
        Ok(())
    }

    async fn init_session(&self, az_topics: TopicFilter) -> Result<(), anyhow::Error> {
        mqtt_channel::init_session(&self.get_mqtt_config()?.with_subscriptions(az_topics)).await?;
        Ok(())
    }

    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();
        let mut mapper =
            create_mapper(AZURE_MAPPER_NAME, mqtt_host, mqtt_port, get_converter()?).await?;
        mapper
            .run()
            .instrument(info_span!(AZURE_MAPPER_NAME))
            .await?;

        Ok(())
    }
}

fn get_converter() -> Result<Box<AzureConverter>, anyhow::Error> {
    let config_repository =
        tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
    let tedge_config = config_repository.load()?;
    let add_timestamp = tedge_config.query(AzureMapperTimestamp)?.is_set();
    let clock = Box::new(WallClock);
    let size_threshold = SizeThreshold(255 * 1024);
    Ok(Box::new(AzureConverter::new(
        add_timestamp,
        clock,
        size_threshold,
    )))
}
