use crate::bridge::BridgeLocation;
use camino::Utf8PathBuf;
use tedge_config::OptionalConfigError;

use super::create::CreateCertCmd;
use super::create_csr::CreateCsrCmd;
use super::remove::RemoveCertCmd;
use super::renew::RenewCertCmd;
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

    /// Create a certificate signing request
    CreateCsr {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id")]
        id: Option<String>,

        /// Path where a Certificate signing request will be stored
        #[clap(long = "output-path")]
        output_path: Option<Utf8PathBuf>,
    },

    /// Renew the device certificate
    Renew,

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
        let config = context.load_config()?;
        let bridge_location = if config.mqtt.bridge.built_in {
            BridgeLocation::BuiltIn
        } else {
            BridgeLocation::Mosquitto
        };

        let cmd = match self {
            TEdgeCertCli::Create { id } => {
                let cmd = CreateCertCmd {
                    id,
                    cert_path: config.device.cert_path.clone(),
                    key_path: config.device.key_path.clone(),
                    csr_path: None,
                    bridge_location,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::CreateCsr { id, output_path } => {
                let cmd = CreateCsrCmd {
                    id,
                    cert_path: config.device.cert_path.clone(),
                    key_path: config.device.key_path.clone(),
                    // Use output file instead of csr_path from tedge config if provided
                    csr_path: output_path.unwrap_or_else(|| config.device.csr_path.clone()),
                    bridge_location,
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
            TEdgeCertCli::Renew => {
                let cmd = RenewCertCmd {
                    cert_path: config.device.cert_path.clone(),
                    key_path: config.device.key_path.clone(),
                    bridge_location,
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
        #[arg(env = "C8Y_USER", default_value = "")]
        /// Provided username should be a Cumulocity user with tenant management permissions.
        /// The username is requested on stdin unless provided by the --user flag, or the C8Y_USER env var.
        username: String,
    },
}
