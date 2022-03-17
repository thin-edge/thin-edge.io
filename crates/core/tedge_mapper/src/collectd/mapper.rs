use crate::{
    collectd::monitor::{DeviceMonitor, DeviceMonitorConfig},
    core::component::TEdgeComponent,
};
use async_trait::async_trait;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting, TEdgeConfig,
};
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

    async fn init(&self) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper collectd");
        mqtt_channel::init_session(&get_mqtt_config()?).await?;
        Ok(())
    }

    async fn clear_session(&self) -> Result<(), anyhow::Error> {
        info!("Clear tedge mapper collectd session");
        mqtt_channel::clear_session(&get_mqtt_config()?).await?;
        Ok(())
    }
}

fn get_mqtt_config() -> Result<mqtt_channel::Config, anyhow::Error> {
    let config_repository =
        tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
    let tedge_config = config_repository.load()?;

    let mqtt_config = mqtt_channel::Config::default()
        .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
        .with_port(tedge_config.query(MqttPortSetting)?.into())
        .with_session_name(COLLECTD_MAPPER_NAME)
        .with_clean_session(false);

    Ok(mqtt_config)
}
