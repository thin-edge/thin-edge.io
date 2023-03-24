use std::path::Path;

use crate::az::converter::AzureConverter;
use crate::core::mapper::create_mapper;
use crate::core::size_threshold::SizeThreshold;
use tedge_mapper_core::component::TEdgeComponent;

use async_trait::async_trait;
use clock::WallClock;
use tedge_config::AzureMapperTimestamp;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_utils::file::create_directory_with_user_group;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

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

    async fn init(&self, config_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper az");
        create_directory_with_user_group(
            format!("{}/operations/az", config_dir.display()),
            "tedge",
            "tedge",
            0o775,
        )?;

        self.init_session(AzureConverter::in_topic_filter()).await?;
        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let add_timestamp = tedge_config.query(AzureMapperTimestamp)?.is_set();
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
        let clock = Box::new(WallClock);
        let size_threshold = SizeThreshold(255 * 1024);

        let converter = Box::new(AzureConverter::new(add_timestamp, clock, size_threshold));

        let mut mapper = create_mapper(AZURE_MAPPER_NAME, mqtt_host, mqtt_port, converter).await?;

        mapper
            .run(Some(&config_dir.join("operations/az")))
            .instrument(info_span!(AZURE_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
