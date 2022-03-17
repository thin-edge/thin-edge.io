use crate::{
    az::converter::AzureConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use async_trait::async_trait;
use clock::WallClock;
use tedge_config::ConfigRepository;
use tedge_config::{AzureMapperTimestamp, MqttBindAddressSetting, TEdgeConfig};
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

    async fn init(&self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper az");
        create_directory_with_user_group(
            "/etc/tedge/operations/az",
            "tedge-mapper",
            "tedge-mapper",
            0o775,
        )?;
        mqtt_channel::init_session(&get_mqtt_config()?).await?;
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), anyhow::Error> {
        info!("Clear tedge mapper session");
        mqtt_channel::clear_session(&get_mqtt_config()?).await?;
        Ok(())
    }
}

fn get_mqtt_config() -> Result<mqtt_channel::Config, anyhow::Error> {
    let config_repository =
        tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
    let tedge_config = config_repository.load()?;

    let mqtt_config = mqtt_channel::Config::default()
        .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
        .with_port(tedge_config.query(MqttPortSetting)?.into())
        .with_session_name(AZURE_MAPPER_NAME)
        .with_clean_session(false);

    Ok(mqtt_config)
}
