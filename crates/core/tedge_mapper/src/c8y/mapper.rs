use crate::{
    c8y::converter::CumulocityConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use agent_interface::topic::ResponseTopic;
use async_trait::async_trait;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::operations::Operations;
use mqtt_channel::TopicFilter;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, DeviceIdSetting, DeviceTypeSetting,
    MqttBindAddressSetting, MqttPortSetting, TEdgeConfig,
};
use tracing::{info, info_span, Instrument};

use super::topic::C8yTopic;

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";
const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16 * 1024;

pub struct CumulocityMapper {}

impl CumulocityMapper {
    pub fn new() -> CumulocityMapper {
        CumulocityMapper {}
    }

    pub fn subscriptions(operations: &Operations) -> Result<TopicFilter, anyhow::Error> {
        let mut topic_filter = TopicFilter::new(ResponseTopic::SoftwareListResponse.as_str())?;
        topic_filter.add(ResponseTopic::SoftwareUpdateResponse.as_str())?;
        topic_filter.add(C8yTopic::SmartRestRequest.as_str())?;
        topic_filter.add(ResponseTopic::RestartResponse.as_str())?;

        for topic in operations.topics_for_operations() {
            topic_filter.add(&topic)?
        }

        Ok(topic_filter)
    }

    pub async fn init_session(&mut self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper session");
        mqtt_channel::init_session(&self.get_mqtt_config()?).await?;
        Ok(())
    }

    pub async fn clear_session(&mut self) -> Result<(), anyhow::Error> {
        info!("Clear tedge mapper session");
        mqtt_channel::clear_session(&self.get_mqtt_config()?).await?;
        Ok(())
    }

    fn get_mqtt_config(&mut self) -> Result<mqtt_channel::Config, anyhow::Error> {
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mqtt_topic = Self::subscriptions(&operations)?;
        let config_repository = tedge_config::TEdgeConfigRepository::new(
            tedge_config::TEdgeConfigLocation::from_default_system_location(),
        );
        let tedge_config = config_repository.load()?;

        let mqtt_config = mqtt_channel::Config::default()
            .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
            .with_port(tedge_config.query(MqttPortSetting)?.into())
            .with_session_name(CUMULOCITY_MAPPER_NAME)
            .with_clean_session(false)
            .with_subscriptions(mqtt_topic);

        Ok(mqtt_config)
    }
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);

        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mut http_proxy = JwtAuthHttpProxy::try_new(&tedge_config).await?;
        http_proxy.init().await?;
        let device_name = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();

        let converter = Box::new(CumulocityConverter::new(
            size_threshold,
            device_name,
            device_type,
            operations,
            http_proxy,
        ));

        let mut mapper =
            create_mapper(CUMULOCITY_MAPPER_NAME, mqtt_host, mqtt_port, converter).await?;

        mapper
            .run()
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
