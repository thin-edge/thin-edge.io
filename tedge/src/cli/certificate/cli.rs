use super::{create::CreateCertCmd, remove::RemoveCertCmd, show::ShowCertCmd, upload::*};

use crate::command::{BuildCommand, BuildContext, Command};
use crate::ConfigError;

use structopt::StructOpt;
use tedge_config::*;

#[derive(StructOpt, Debug)]
pub enum TEdgeCertCli {
    /// Create a self-signed device certificate
    Create {
        /// The device identifier to be used as the common name for the certificate
        #[structopt(long = "device-id")]
        id: String,
    },

    /// Show the device certificate, if any
    Show,

    /// Remove the device certificate
    Remove,

    /// Upload root certificate
    Upload(UploadCertCli),
}

impl BuildCommand for TEdgeCertCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.config_repository.load()?;

        let cmd = match self {
            TEdgeCertCli::Create { id } => {
                let cmd = CreateCertCmd {
                    id,
                    cert_path: config.query(DeviceCertPathSetting)?,
                    key_path: config.query(DeviceKeyPathSetting)?,
                    config_repository: context.config_repository,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Show => {
                let cmd = ShowCertCmd {
                    cert_path: config.query(DeviceCertPathSetting)?,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Remove => {
                let cmd = RemoveCertCmd {
                    cert_path: config.query(DeviceCertPathSetting)?,
                    key_path: config.query(DeviceKeyPathSetting)?,
                    config_repository: context.config_repository,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Upload(cmd) => cmd.build_command(context)?,
        };

        Ok(cmd)
    }
}

#[derive(StructOpt, Debug)]
pub enum UploadCertCli {
    /// Upload root certificate to Cumulocity
    ///
    /// The command will upload root certificate to Cumulocity.
    C8y {
        #[structopt(long = "user")]
        /// Provided username should be a Cumulocity user with tenant management permissions.
        /// The password is requested on /dev/tty, unless the $C8YPASS env var is set to the user password.
        username: String,
    },
}

impl BuildCommand for UploadCertCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.config_repository.load()?;

        match self {
            UploadCertCli::C8y { username } => Ok((UploadCertCmd {
                device_id: config.query(DeviceIdSetting)?,
                path: config.query(DeviceCertPathSetting)?,
                host: config.query(C8yUrlSetting)?,
                username,
            })
            .into_boxed()),
        }
    }
}
