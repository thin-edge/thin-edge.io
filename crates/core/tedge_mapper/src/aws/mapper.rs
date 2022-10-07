use std::path::Path;

use crate::{
    aws::converter::AwsConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use async_trait::async_trait;
use clock::WallClock;
use tedge_config::{AwsMapperTimestamp, MqttBindAddressSetting, TEdgeConfig};
use tedge_config::{ConfigSettingAccessor, MqttPortSetting};
use tedge_utils::file::create_directory_with_user_group;
use tracing::{info, info_span, Instrument};

const AWS_MAPPER_NAME: &str = "tedge-mapper-aws";

pub struct AwsMapper {}

impl AwsMapper {
    pub fn new() -> AwsMapper {
        AwsMapper {}
    }
}

#[async_trait]
impl TEdgeComponent for AwsMapper {
    fn session_name(&self) -> &str {
        AWS_MAPPER_NAME
    }

    async fn init(&self, config_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper aws");
        create_directory_with_user_group(
            format!("{}/operations/aws", config_dir.display()),
            "tedge",
            "tedge",
            0o775,
        )?;

        self.init_session(AwsConverter::in_topic_filter()).await?;
        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let add_timestamp = tedge_config.query(AwsMapperTimestamp)?.is_set();
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();
        let clock = Box::new(WallClock);
        let size_threshold = SizeThreshold(255 * 1024);

        let converter = Box::new(AwsConverter::new(add_timestamp, clock, size_threshold));

        let mut mapper = create_mapper(AWS_MAPPER_NAME, mqtt_host, mqtt_port, converter).await?;

        mapper
            .run(None)
            .instrument(info_span!(AWS_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
