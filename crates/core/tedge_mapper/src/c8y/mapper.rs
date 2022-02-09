use crate::{
    c8y::converter::CumulocityConverter,
    mapping::{
        component::TEdgeComponent, mapper::create_mapper, operations::Operations,
        size_threshold::SizeThreshold,
    },
};

use agent_interface::topic::ResponseTopic;
use async_trait::async_trait;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use mqtt_channel::{Config, TopicFilter};
use tedge_config::{
    ConfigSettingAccessor, DeviceIdSetting, DeviceTypeSetting, MqttPortSetting, TEdgeConfig,
};
use tracing::{info, info_span, Instrument};

use super::topic::C8yTopic;

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";

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
        info!("Initialize tedge sm mapper session");
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mqtt_topic = Self::subscriptions(&operations)?;
        let config = Config::default()
            .with_session_name(CUMULOCITY_MAPPER_NAME)
            .with_clean_session(false)
            .with_subscriptions(mqtt_topic);
        mqtt_channel::init_session(&config).await?;
        Ok(())
    }

    pub async fn clear_session(&mut self) -> Result<(), anyhow::Error> {
        info!("Clear tedge sm mapper session");
        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let mqtt_topic = Self::subscriptions(&operations)?;
        let config = Config::default()
            .with_session_name(CUMULOCITY_MAPPER_NAME)
            .with_clean_session(true)
            .with_subscriptions(mqtt_topic);
        mqtt_channel::clear_session(&config).await?;
        Ok(())
    }
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(16 * 1024);

        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;
        let http_proxy = JwtAuthHttpProxy::try_new(&tedge_config, CUMULOCITY_MAPPER_NAME).await?;
        let device_name = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();

        let converter = Box::new(CumulocityConverter::new(
            size_threshold,
            device_name,
            device_type,
            operations,
            http_proxy,
        ));

        let mut mapper = create_mapper(CUMULOCITY_MAPPER_NAME, mqtt_port, converter).await?;

        mapper
            .run()
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
