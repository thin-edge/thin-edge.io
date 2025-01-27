use certificate::parse_root_certificate::CryptokiConfig;

use crate::TEdgeConfigReaderDeviceCryptoki;

impl TEdgeConfigReaderDeviceCryptoki {
    pub fn config(&self) -> Result<Option<CryptokiConfig>, anyhow::Error> {
        if !self.enable {
            return Ok(None);
        }

        Ok(Some(CryptokiConfig {
            module_path: self.module_path.or_config_not_set().unwrap().clone(),
            pin: self.pin.clone(),
            serial: self.serial.or_none().cloned(),
        }))
    }
}
