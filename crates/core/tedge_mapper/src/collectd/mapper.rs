use std::path::Path;

use crate::collectd::monitor::DeviceMonitor;
use crate::collectd::monitor::DeviceMonitorConfig;
use async_trait::async_trait;
use mqtt_channel::TopicFilter;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_mapper_core::component::TEdgeComponent;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

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

    async fn init(&self, _cfg_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper collectd");
        self.init_session(TopicFilter::new(
            DeviceMonitorConfig::default().mqtt_source_topic,
        )?)
        .await?;
        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;

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
