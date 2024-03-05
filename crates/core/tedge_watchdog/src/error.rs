use mqtt_channel::MqttError;
use tedge_config::CertificateError;
use tedge_config::ConfigSettingError;
use tedge_config::TEdgeConfigError;
use time::error::Parse;

#[derive(Debug, thiserror::Error)]
pub enum WatchdogError {
    #[error("MQTT receiver closed")]
    ChannelClosed,

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

    #[error(transparent)]
    FromCertificateError(#[from] CertificateError),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),

    #[error(transparent)]
    ParseError(#[from] Parse),

    #[error(transparent)]
    CustomError(#[from] anyhow::Error),
}
