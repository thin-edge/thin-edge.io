mod batcher;
mod collectd;
mod error;
mod monitor;

use tracing::{debug_span, info, Instrument};

use crate::error::*;
use crate::monitor::{DeviceMonitor, DeviceMonitorConfig};
use std::path::PathBuf;
use tedge_config::*;

const APP_NAME: &str = "tedge-dm-agent";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    info!("{} starting!", APP_NAME);

    let device_monitor_config = DeviceMonitorConfig {
        port: get_mqtt_port()?,
        ..DeviceMonitorConfig::default()
    };
    let device_monitor = DeviceMonitor::new(device_monitor_config);
    device_monitor
        .run()
        .instrument(debug_span!(APP_NAME))
        .await?;

    Ok(())
}

fn get_mqtt_port() -> Result<u16, anyhow::Error> {
    let config_repository = get_config_repository()?;
    let tedge_config = config_repository.load()?;
    Ok(tedge_config.query(MqttPortSetting)?.0)
}

fn get_config_repository() -> Result<TEdgeConfigRepository, anyhow::Error> {
    let tedge_config_location = if running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            home_dir().ok_or(DeviceMonitorError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    Ok(config_repository)
}

// Copied from tedge/src/utils/users/unix.rs. In the future, it would be good to separate it from tedge crate.
fn running_as_root() -> bool {
    users::get_current_uid() == 0
}

// Copied from tedge/src/utils/paths.rs. In the future, it would be good to separate it from tedge crate.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from)
}
