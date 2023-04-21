use mqtt_channel::MqttError;

use tedge_config::ConfigSettingError;
use tedge_config::TEdgeConfigError;

#[derive(Debug, thiserror::Error)]
pub enum WatchdogError {
    #[cfg(not(target_os = "linux"))]
    #[error("The watchdog is not available on this platform")]
    WatchdogNotAvailable,

    #[error("Fail to run `{cmd}`: {from}")]
    CommandExecError { cmd: String, from: std::io::Error },

    #[error(transparent)]
    FromTedgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSettingError(#[from] ConfigSettingError),

    #[error(transparent)]
    FromMqttError(#[from] MqttError),

    #[error(transparent)]
    DeserializeError(#[from] serde_json::Error),

    #[error(transparent)]
    ParseWatchdogSecToInt(#[from] std::num::ParseIntError),

    #[error(transparent)]
    ParseSystemdFile(#[from] std::io::Error),

    #[error("Did not find the WatchdogSec in {file}")]
    NoWatchdogSec { file: String },

    #[error("Error configuring MQTT client")]
    FromMqttConfigBuild(#[from] tedge_config::mqtt_config::MqttConfigBuildError),
}
