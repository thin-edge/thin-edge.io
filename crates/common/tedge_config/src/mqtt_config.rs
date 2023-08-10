use crate::ConfigSettingError;
use certificate::CertificateError;

#[derive(Debug, thiserror::Error)]
pub enum MqttConfigBuildError {
    #[error("Invalid tedge config")]
    InvalidTedgeConfig(#[from] ConfigSettingError),

    #[error("Error setting MQTT config value")]
    FromCertificate(#[from] CertificateError),
}
