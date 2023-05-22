use tedge_config::new::OptionalConfigError;

use super::create::CreateCertCmd;
use super::remove::RemoveCertCmd;
use super::show::ShowCertCmd;
use super::upload::*;

use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::ConfigError;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeCertCli {
    /// Create a self-signed device certificate
    Create {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id")]
        id: String,
    },

    /// Show the device certificate, if any
    Show,

    /// Remove the device certificate
    Remove,

    /// Upload root certificate
    #[clap(subcommand)]
    Upload(UploadCertCli),
}

impl BuildCommand for TEdgeCertCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.config_repository.load_new()?;

        let cmd = match self {
            TEdgeCertCli::Create { id } => {
                let cmd = CreateCertCmd {
                    id,
                    cert_path: config.device.cert_path.clone(),
                    key_path: config.device.key_path.clone(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Show => {
                let cmd = ShowCertCmd {
                    cert_path: config.device.cert_path.clone(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Remove => {
                let cmd = RemoveCertCmd {
                    cert_path: config.device.cert_path.clone(),
                    key_path: config.device.key_path.clone(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Upload(cmd) => {
                let cmd = match cmd {
                    UploadCertCli::C8y { username } => UploadCertCmd {
                        device_id: config.device.id.try_read(&config)?.clone(),
                        path: config.device.cert_path.clone(),
                        host: config.c8y.http.or_err()?.to_owned(),
                        username,
                    },
                };
                cmd.into_boxed()
            }
        };

        Ok(cmd)
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum UploadCertCli {
    /// Upload root certificate to Cumulocity
    ///
    /// The command will upload root certificate to Cumulocity.
    C8y {
        #[clap(long = "user")]
        /// Provided username should be a Cumulocity user with tenant management permissions.
        /// The password is requested on /dev/tty, unless the $C8YPASS env var is set to the user password.
        username: String,
    },
}
