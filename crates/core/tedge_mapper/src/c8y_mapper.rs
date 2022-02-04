use crate::c8y_converter::CumulocityConverter;
use crate::component::TEdgeComponent;
use crate::mapper::*;
use crate::size_threshold::SizeThreshold;
use async_trait::async_trait;
use tedge_config::{
    ConfigSettingAccessor, DeviceIdSetting, DeviceTypeSetting, MqttPortSetting, TEdgeConfig,
};
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

        let device_name = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();

        let converter = Box::new(CumulocityConverter::new(
            size_threshold,
            device_name,
            device_type,
        ));

        let mut mapper = create_mapper(CUMULOCITY_MAPPER_NAME, mqtt_port, converter).await?;

        mapper
            .run()
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
