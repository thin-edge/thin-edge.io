use crate::ConfigSettingAccessor;
use crate::ConfigSettingError;
use crate::MqttClientAuthCertSetting;
use crate::MqttClientAuthKeySetting;
use crate::MqttClientCafileSetting;
use crate::MqttClientCapathSetting;
use crate::MqttClientHostSetting;
use crate::MqttClientPortSetting;
use crate::TEdgeConfig;
use certificate::CertificateError;

// TODO!: Remove this after replacing tedge config API by the new one.
impl TEdgeConfig {
    pub fn mqtt_config(&self) -> Result<mqtt_channel::Config, MqttConfigBuildError> {
        let host = self.query(MqttClientHostSetting)?;
        let port = self.query(MqttClientPortSetting)?;
        let mut mqtt_config = mqtt_channel::Config::default()
            .with_host(host)
            .with_port(port.into());

        // If these options are not set, just dont use them
        let ca_file = self.query(MqttClientCafileSetting).ok();
        let ca_path = self.query(MqttClientCapathSetting).ok();

        // Both these options have to either be set or not set, so we keep
        // original error to rethrow when only one is set
        let client_cert = self.query(MqttClientAuthCertSetting);
        let client_key = self.query(MqttClientAuthKeySetting);

        // Configure certificate authentication
        if let Some(ca_file) = ca_file {
            mqtt_config.with_cafile(ca_file)?;
        }

        if let Some(ca_path) = ca_path {
            mqtt_config.with_cadir(ca_path)?;
        }

        // TODO (Marcel): remove unnecessary error checks once tedge_config
        // refactor lands
        if client_cert.is_ok() || client_key.is_ok() {
            let client_cert = client_cert?;
            let client_key = client_key?;

            mqtt_config.with_client_auth(client_cert, client_key)?;
        }

        Ok(mqtt_config)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MqttConfigBuildError {
    #[error("Invalid tedge config")]
    InvalidTedgeConfig(#[from] ConfigSettingError),

    #[error("Error setting MQTT config value")]
    FromCertificate(#[from] CertificateError),
}
