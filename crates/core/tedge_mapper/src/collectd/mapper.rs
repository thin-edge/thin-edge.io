use crate::{
    collectd::monitor::{DeviceMonitor, DeviceMonitorConfig},
    core::component::TEdgeComponent,
};
use async_trait::async_trait;
use mqtt_channel::TopicFilter;
use tedge_config::{ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting, TEdgeConfig};
use tracing::{info, info_span, Instrument};

const COLLECTD_MAPPER_NAME: &str = "tedge-mapper-collectd";

pub struct CollectdMapper {}

impl CollectdMapper {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl TEdgeComponent for CollectdMapper {
    fn session_name(&self) -> &str {
        COLLECTD_MAPPER_NAME
    }

    async fn init(&self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper collectd");
        self.init_session(TopicFilter::new(
            DeviceMonitorConfig::default().mqtt_source_topic,
        )?)
        .await?;
        Ok(())
    }

    async fn init_session(&self, c8y_topics: TopicFilter) -> Result<(), anyhow::Error> {
        mqtt_channel::init_session(&self.get_mqtt_config()?.with_subscriptions(c8y_topics)).await?;
        Ok(())
    }

    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();

        let device_monitor_config = DeviceMonitorConfig::default()
            .with_port(mqtt_port)
            .with_host(mqtt_host);

        let device_monitor = DeviceMonitor::new(device_monitor_config);
        device_monitor
            .run()
            .instrument(info_span!(COLLECTD_MAPPER_NAME))
            .await?;

        Ok(())
    }
}
