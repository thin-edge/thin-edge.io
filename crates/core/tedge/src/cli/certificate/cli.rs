use super::create::CreateCertCmd;
use super::create_csr::CreateCsrCmd;
use super::remove::RemoveCertCmd;
use super::renew::RenewCertCmd;
use super::show::ShowCertCmd;
use crate::cli::certificate::c8y;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use camino::Utf8PathBuf;
use std::time::Duration;
use tedge_config::OptionalConfigError;
use tedge_config::ProfileName;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeCertCli {
    /// Create a self-signed device certificate
    Create {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id")]
        id: String,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Create a certificate signing request
    CreateCsr {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id", global = true)]
        id: Option<String>,

        /// Path where a Certificate signing request will be stored
        #[clap(long = "output-path", global = true)]
        output_path: Option<Utf8PathBuf>,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Renew the device certificate
    Renew {
        /// CA from which the certificate will be renew
        #[arg(value_enum, default_value = "self-signed")]
        ca: CertRenewalCA,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Show the device certificate, if any
    Show {
        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Remove the device certificate
    Remove {
        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Upload root certificate
    #[clap(subcommand)]
    Upload(UploadCertCli),

    /// Request and download the device certificate
    #[clap(subcommand)]
    Download(DownloadCertCli),
}

impl BuildCommand for TEdgeCertCli {
    fn build_command(self, context: BuildContext) -> Result<Box<dyn Command>, ConfigError> {
        let config = context.load_config()?;
        let (user, group) = if config.mqtt.bridge.built_in {
            ("tedge", "tedge")
        } else {
            (crate::BROKER_USER, crate::BROKER_USER)
        };

        let cmd = match self {
            TEdgeCertCli::Create { id, cloud } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cmd = CreateCertCmd {
                    id,
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    user: user.to_owned(),
                    group: group.to_owned(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::CreateCsr {
                id,
                output_path,
                cloud,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                // Use the current device id if no id is provided
                let id = match id {
                    Some(id) => id,
                    None => config.device.id()?.clone(),
                };
                let cmd = CreateCsrCmd {
                    id,
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    // Use output file instead of csr_path from tedge config if provided
                    csr_path: output_path.unwrap_or_else(|| config.device.csr_path.clone()),
                    user: user.to_owned(),
                    group: group.to_owned(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Show { cloud } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cmd = ShowCertCmd {
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Remove { cloud } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cmd = RemoveCertCmd {
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Upload(UploadCertCli::C8y {
                username,
                password,
                profile,
            }) => {
                let c8y = config.c8y.try_get(profile.as_deref())?;
                let cmd = c8y::UploadCertCmd {
                    device_id: c8y.device.id()?.clone(),
                    path: c8y.device.cert_path.clone(),
                    host: c8y.http.or_err()?.to_owned(),
                    cloud_root_certs: config.cloud_root_certs(),
                    username,
                    password,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Download(DownloadCertCli::C8y {
                id,
                token,
                profile,
                retry_every,
                max_timeout,
            }) => {
                let c8y_config = config.c8y.try_get(profile.as_deref())?;
                let cmd = c8y::DownloadCertCmd {
                    device_id: id,
                    security_token: token,
                    c8y_url: c8y_config.http.or_err()?.to_owned(),
                    root_certs: config.cloud_root_certs(),
                    cert_path: c8y_config.device.cert_path.to_owned(),
                    key_path: c8y_config.device.key_path.to_owned(),
                    csr_path: c8y_config.device.csr_path.to_owned(),
                    retry_every,
                    max_timeout,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Renew {
                ca: CertRenewalCA::SelfSigned,
                cloud,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cmd = RenewCertCmd {
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Renew {
                ca: CertRenewalCA::C8y,
                cloud,
            } => {
                let c8y_config = match cloud.map(<_>::try_into).transpose()? {
                    None => config.c8y.try_get::<str>(None)?,
                    Some(Cloud::C8y(profile)) => config.c8y.try_get(profile.as_deref())?,
                    Some(cloud) => {
                        return Err(
                            anyhow!("Certificate renewal is not supported for {cloud}").into()
                        )
                    }
                };
                let cmd = c8y::RenewCertCmd {
                    device_id: c8y_config.device.id()?.to_string(),
                    c8y_url: c8y_config.http.or_err()?.to_owned(),
                    root_certs: config.cloud_root_certs(),
                    cert_path: c8y_config.device.cert_path.clone(),
                    key_path: c8y_config.device.key_path.clone(),
                    csr_path: c8y_config.device.csr_path.clone(),
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
        #[arg(
            env = "C8Y_USER",
            hide_env_values = true,
            hide_default_value = true,
            default_value = ""
        )]
        /// Provided username should be a Cumulocity IoT user with tenant management permissions.
        /// You will be prompted for input if the value is not provided or is empty
        username: String,

        #[clap(long = "password")]
        #[arg(env = "C8Y_PASSWORD", hide_env_values = true, hide_default_value = true, default_value_t = std::env::var("C8YPASS").unwrap_or_default().to_string())]
        // Note: Prefer C8Y_PASSWORD over the now deprecated C8YPASS env variable as the former is also supported by other tooling such as go-c8y-cli
        /// Cumulocity IoT Password.
        /// You will be prompted for input if the value is not provided or is empty
        ///
        /// Notes: `C8YPASS` is deprecated. Please use the `C8Y_PASSWORD` env variable instead
        password: String,

        /// The cloud profile you wish to upload the certificate to
        #[clap(long)]
        profile: Option<ProfileName>,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum DownloadCertCli {
    #[clap(verbatim_doc_comment)]
    /// Request and download the device certificate from Cumulocity
    ///
    /// - Generate a private key and Signing Certificate Request (CSR) for the device
    /// - Upload this CSR on Cumulocity, using the provided device identifier and security token
    /// - Loop till the device is registered by an administrator and the CSR accepted
    /// - Store the certificate created by Cumulocity
    ///
    /// Use the following settings from the config:
    /// - c8y.http  HTTP Endpoint to the Cumulocity tenant, with optional port
    /// - device.key_path  Path where the device's private key is stored
    /// - device.cert_path  Path where the device's certificate is stored
    /// - device.csr_path  Path where the device's certificate signing request is stored
    C8y {
        /// The device identifier to be used as the common name for the certificate
        ///
        /// You will be prompted for input if the value is not provided or is empty
        #[clap(long = "device-id")]
        #[arg(
            env = "DEVICE_ID",
            hide_env_values = true,
            hide_default_value = true,
            default_value = ""
        )]
        id: String,

        #[clap(long)]
        #[arg(
            env = "DEVICE_TOKEN",
            hide_env_values = true,
            hide_default_value = true,
            default_value = ""
        )]
        /// The security token assigned to this device when registered to Cumulocity
        ///
        /// You will be prompted for input if the value is not provided or is empty
        token: String,

        #[clap(long)]
        /// The Cumulocity cloud profile (when the device is connected to several tenants)
        profile: Option<ProfileName>,

        #[clap(long, default_value = "30s")]
        #[arg(value_parser = humantime::parse_duration)]
        /// Delay between two attempts, polling till the device is registered
        retry_every: Duration,

        #[clap(long, default_value = "10m")]
        #[arg(value_parser = humantime::parse_duration)]
        /// Maximum time waiting for the device to be registered
        max_timeout: Duration,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum CertRenewalCA {
    /// Self-signed a new device certificate
    SelfSigned,

    /// Renew the device certificate from Cumulocity
    C8y,
}
