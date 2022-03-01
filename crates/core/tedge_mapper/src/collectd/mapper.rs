use crate::{
    collectd::monitor::{DeviceMonitor, DeviceMonitorConfig},
    core::component::TEdgeComponent,
};
use async_trait::async_trait;
use tedge_config::{ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting, TEdgeConfig};
use tracing::{info_span, Instrument};

const APP_NAME: &str = "tedge-mapper-collectd";

pub struct CollectdMapper {}

impl CollectdMapper {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl TEdgeComponent for CollectdMapper {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();

        let device_monitor_config = DeviceMonitorConfig::default()
            .with_port(mqtt_port)
            .with_host(mqtt_host);

        let device_monitor = DeviceMonitor::new(device_monitor_config);
        device_monitor
            .run()
            .instrument(info_span!(APP_NAME))
            .await?;

        Ok(())
    }
}
