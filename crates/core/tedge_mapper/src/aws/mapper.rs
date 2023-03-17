use std::path::Path;

use crate::aws::converter::AwsConverter;
use crate::core::mapper::create_mapper;
use crate::core::size_threshold::SizeThreshold;
use tedge_mapper_core::component::TEdgeComponent;

use async_trait::async_trait;
use clock::WallClock;
use tedge_config::AwsMapperTimestamp;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

const AWS_MAPPER_NAME: &str = "tedge-mapper-aws";

pub struct AwsMapper;

#[async_trait]
impl TEdgeComponent for AwsMapper {
    fn session_name(&self) -> &str {
        AWS_MAPPER_NAME
    }

    async fn init(&self, _config_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper aws");
        self.init_session(AwsConverter::in_topic_filter()).await?;

        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let add_timestamp = tedge_config.query(AwsMapperTimestamp)?.is_set();
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
        let clock = Box::new(WallClock);
        // Quotas at: https://docs.aws.amazon.com/general/latest/gr/iot-core.html#limits_iot
        let size_threshold = SizeThreshold(128 * 1024);

        let converter = Box::new(AwsConverter::new(add_timestamp, clock, size_threshold));

        let mut mapper = create_mapper(AWS_MAPPER_NAME, mqtt_host, mqtt_port, converter).await?;

        mapper
            .run(None)
            .instrument(info_span!(AWS_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
