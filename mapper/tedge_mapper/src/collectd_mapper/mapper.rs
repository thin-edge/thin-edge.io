use crate::{
    collectd_mapper::monitor::{DeviceMonitor, DeviceMonitorConfig},
    component::TEdgeComponent,
};
use async_trait::async_trait;
use tedge_config::{ConfigSettingAccessor, MqttPortSetting, TEdgeConfig};
use tracing::{debug_span, Instrument};

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

        let device_monitor_config = DeviceMonitorConfig::default().with_port(mqtt_port);

        let device_monitor = DeviceMonitor::new(device_monitor_config);
        device_monitor
            .run()
            .instrument(debug_span!(APP_NAME))
            .await?;

        Ok(())
    }
}
