use std::sync::Arc;

use anyhow::bail;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::CryptokiConfig;

use crate::TEdgeConfigReaderDeviceCryptoki;

pub enum CryptokiOpts {
    Disabled,

    Enabled {
        module_path: Utf8PathBuf,
        pin: Arc<str>,
        serial: Option<Arc<str>>,
    },
}

impl TEdgeConfigReaderDeviceCryptoki {
    pub fn opts(&self) -> Result<CryptokiOpts, anyhow::Error> {
        if !self.enable {
            return Ok(CryptokiOpts::Disabled);
        }

        Ok(CryptokiOpts::Enabled {
            module_path: self.module_path.or_config_not_set()?.clone(),
            pin: self.pin.clone(),
            serial: self.serial.or_none().cloned(),
        })
    }

    pub fn config(&self) -> Result<CryptokiConfig, anyhow::Error> {
        if !self.enable {
            bail!("disabled");
        }

        Ok(CryptokiConfig {
            module_path: self.module_path.or_config_not_set().unwrap().clone(),
            pin: self.pin.clone(),
            serial: self.serial.or_none().cloned(),
        })
    }
}

impl TryFrom<CryptokiOpts> for CryptokiConfig {
    type Error = anyhow::Error;

    fn try_from(value: CryptokiOpts) -> Result<Self, Self::Error> {
        let CryptokiOpts::Enabled {
            module_path,
            pin,
            serial,
        } = value
        else {
            // TODO(marcel): eliminate error cases!
            anyhow::bail!("disabled");
        };

        Ok(CryptokiConfig {
            module_path,
            pin,
            serial,
        })
    }
}

// impl TryFrom<&TEdgeConfigReaderDeviceCryptoki> for CryptokiOpts {
//     type Error = anyhow::Error;

//     fn try_from(reader: &TEdgeConfigReaderDeviceCryptoki) -> Result<Self, Self::Error> {
//         if !reader.enable {
//             return Ok(Self::Disabled);
//         }

//         Ok(Self::Enabled {
//             module_path: reader.module_path.or_config_not_set()?.clone(),
//             pin: reader.pin.clone(),
//             serial: reader.serial.or_config_not_set()?.clone(),
//         })
//     }
// }

// impl TryFrom<&TEdgeConfig> for CryptokiOpts {
//     type Error = anyhow::Error;

//     fn try_from(tedge_config: &TEdgeConfig) -> Result<Self, Self::Error> {
//         (&tedge_config.device.cryptoki).try_into()
//     }
// }
