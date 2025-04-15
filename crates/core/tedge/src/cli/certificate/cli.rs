use super::create::CreateCertCmd;
use super::create_csr::CreateCsrCmd;
use super::remove::RemoveCertCmd;
use super::renew::RenewCertCmd;
use super::show::ShowCertCmd;
use crate::certificate_is_self_signed;
use crate::cli::certificate::c8y;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::ConfigError;
use anyhow::anyhow;
use c8y_api::http_proxy::C8yEndPoint;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use clap::ValueHint;
use std::time::Duration;
use tedge_config::tedge_toml::OptionalConfigError;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;

#[derive(clap::Subcommand, Debug)]
pub enum TEdgeCertCli {
    /// Create a self-signed device certificate
    Create {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id", global = true)]
        id: Option<String>,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Create a certificate signing request
    CreateCsr {
        /// The device identifier to be used as the common name for the certificate
        #[clap(long = "device-id", global = true)]
        id: Option<String>,

        /// Path where a Certificate signing request will be stored
        #[clap(long = "output-path", global = true, value_hint = ValueHint::FilePath)]
        output_path: Option<Utf8PathBuf>,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Renew the device certificate
    Renew {
        /// Path to a Certificate Signing Request (CSR) ready to be used
        ///
        /// Providing the CSR is notably required when the request has to be signed
        /// by a tier tool owning the private key of the device.
        ///
        /// If none is provided a CSR is generated using the device id and private key
        /// configured for the given cloud profile.
        #[clap(long = "csr-path", global = true, value_hint = ValueHint::FilePath)]
        csr_path: Option<Utf8PathBuf>,

        /// Force the renewal of self-signed certificates as self-signed
        ///
        /// This can be used to bypass the default behavior
        /// which is to forward the renewal request to the cloud CA
        /// even if the current certificate has not been signed by this CA.
        /// In most cases, the default behavior is what you want:
        /// substitute a proper CA-signed certificate for a self-signed certificate.
        ///
        /// However, if this is not the case, or if the cloud endpoint doesn't provide a CA:
        /// use `--self-signed` to get a renewed self-signed certificate.
        #[clap(long = "self-signed", default_value_t = false)]
        self_signed_only: bool,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Check if the device certificate has to be renewed
    ///
    /// Exit code:
    /// * `0` - certificate needs renewal as it is no longer valid,
    ///   or it will expire within the duration, `certificate.validity.minimum_duration`
    /// * `1` - certificate is still valid and does not need renewal
    /// * `2` - unexpected error (e.g. certificate does not exist, or can't be read)
    NeedsRenewal {
        /// Path to the certificate - default to the configured device certificate
        #[clap(long = "cert-path", value_hint = ValueHint::FilePath)]
        cert_path: Option<Utf8PathBuf>,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Show the device certificate, if any
    Show {
        /// Path to the certificate - default to the configured device certificate
        #[clap(long = "cert-path", value_hint = ValueHint::FilePath)]
        cert_path: Option<Utf8PathBuf>,

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
    fn build_command(
        self,
        config: TEdgeConfig,
        _: TEdgeConfigLocation,
    ) -> Result<Box<dyn Command>, ConfigError> {
        let (user, group) = if config.mqtt.bridge.built_in {
            ("tedge", "tedge")
        } else {
            (crate::BROKER_USER, crate::BROKER_USER)
        };

        let csr_template = CsrTemplate {
            max_cn_size: 64,
            validity_period_days: config
                .certificate
                .validity
                .requested_duration
                .duration()
                .as_secs() as u32
                / (24 * 3600),
            organization_name: config.certificate.organization.to_string(),
            organizational_unit_name: config.certificate.organization_unit.to_string(),
        };

        let cmd = match self {
            TEdgeCertCli::Create { id, cloud } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;

                let cmd = CreateCertCmd {
                    id: get_device_id(id, &config, &cloud)?,
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    user: user.to_owned(),
                    group: group.to_owned(),
                    csr_template,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::CreateCsr {
                id,
                output_path,
                cloud,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;

                let cmd = CreateCsrCmd {
                    id: get_device_id(id, &config, &cloud)?,
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    // Use output file instead of csr_path from tedge config if provided
                    csr_path: if let Some(output_path) = output_path {
                        output_path
                    } else {
                        config.device_csr_path(cloud.as_ref())?.to_owned()
                    },
                    user: user.to_owned(),
                    group: group.to_owned(),
                    csr_template,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Show { cloud, cert_path } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let device_cert_path = config.device_cert_path(cloud.as_ref())?.to_owned();
                let cmd = ShowCertCmd {
                    cert_path: cert_path.unwrap_or(device_cert_path),
                    minimum: config.certificate.validity.minimum_duration.duration(),
                    validity_check_only: false,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::NeedsRenewal { cloud, cert_path } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let device_cert_path = config.device_cert_path(cloud.as_ref())?.to_owned();
                let cmd = ShowCertCmd {
                    cert_path: cert_path.unwrap_or(device_cert_path),
                    minimum: config.certificate.validity.minimum_duration.duration(),
                    validity_check_only: true,
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
                one_time_password: token,
                profile,
                csr_path,
                retry_every,
                max_timeout,
            }) => {
                let c8y_config = config.c8y.try_get(profile.as_deref())?;

                let (csr_path, generate_csr) = match csr_path {
                    None => (c8y_config.device.csr_path.clone(), true),
                    Some(csr_path) => (csr_path, false),
                };

                let cmd = c8y::DownloadCertCmd {
                    device_id: id,
                    one_time_password: token,
                    c8y_url: c8y_config.http.or_err()?.to_owned(),
                    root_certs: config.cloud_root_certs(),
                    cert_path: c8y_config.device.cert_path.to_owned(),
                    key_path: c8y_config.device.key_path.to_owned(),
                    csr_path,
                    generate_csr,
                    retry_every,
                    max_timeout,
                    csr_template,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::Renew {
                csr_path,
                cloud,
                self_signed_only,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cert_path = config.device_cert_path(cloud.as_ref())?.to_owned();
                let key_path = config.device_key_path(cloud.as_ref())?.to_owned();

                // The CA to renew a certificate is determined from the certificate
                //
                // The current implementation is simplified knowing that `tedge cert renew`
                // only supports self-signed certificates and certificates signed by c8y
                // and more precisely by the CA of the c8y tenant the device is connected to.
                //
                // - if the certificate is self_signed and self_signed_only is set => create a new self-signed certificate
                // - if the certificate is self_signed but self_signed_only is not set => try to use the CA of the tenant
                // - if not => assume that the device tenant and its CA tenant are the same.
                let is_self_signed = match certificate_is_self_signed(&cert_path) {
                    Ok(is_self_signed) => is_self_signed,
                    Err(err) => {
                        return Err(anyhow!("Cannot renew certificate {cert_path}: {err}").into())
                    }
                };

                if is_self_signed && self_signed_only {
                    let cmd = RenewCertCmd {
                        cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                        key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                        csr_template,
                    };
                    cmd.into_boxed()
                } else if self_signed_only {
                    return Err(
                        anyhow!("Cannot renew certificate with `--self-signed`: {cert_path} is not self-signed").into()
                    );
                } else {
                    let (csr_path, generate_csr) = match csr_path {
                        None => (config.device_csr_path(cloud.as_ref())?.to_owned(), true),
                        Some(csr_path) => (csr_path, false),
                    };
                    let c8y = match cloud {
                        None => C8yEndPoint::local_proxy(&config, None)?,
                        Some(Cloud::C8y(profile)) => C8yEndPoint::local_proxy(
                            &config,
                            profile.as_deref().map(|p| p.as_ref()),
                        )?,
                        Some(cloud) => {
                            return Err(
                                anyhow!("Certificate renewal is not supported for {cloud}").into()
                            )
                        }
                    };
                    let cmd = c8y::RenewCertCmd {
                        c8y,
                        root_certs: config.cloud_root_certs(),
                        identity: config.http.client.auth.identity()?,
                        cert_path,
                        key_path,
                        csr_path,
                        generate_csr,
                        csr_template,
                    };
                    cmd.into_boxed()
                }
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
        /// Provided username should be a Cumulocity user with tenant management permissions.
        /// You will be prompted for input if the value is not provided or is empty
        username: String,

        #[clap(long = "password")]
        #[arg(env = "C8Y_PASSWORD", hide_env_values = true, hide_default_value = true, default_value_t = std::env::var("C8YPASS").unwrap_or_default().to_string())]
        // Note: Prefer C8Y_PASSWORD over the now deprecated C8YPASS env variable as the former is also supported by other tooling such as go-c8y-cli
        /// Cumulocity Password.
        /// You will be prompted for input if the value is not provided or is empty
        ///
        /// Notes: `C8YPASS` is deprecated. Please use the `C8Y_PASSWORD` env variable instead
        password: String,

        /// The cloud profile you wish to upload the certificate to
        #[clap(long)]
        profile: Option<ProfileName>,
    },
}

/// Returns the device ID from the config if no ID is provided by CLI
fn get_device_id(
    id: Option<String>,
    config: &TEdgeConfig,
    cloud: &Option<Cloud>,
) -> Result<String, anyhow::Error> {
    match (id, config.device_id(cloud.as_ref()).ok()) {
        (None, None) => Err(anyhow!(
            "No device ID is provided. Use `--device-id <name>` option to specify the device ID."
        )),
        (None, Some(config_id)) => Ok(config_id.into()),
        (Some(input_id), _) => Ok(input_id),
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum DownloadCertCli {
    #[clap(verbatim_doc_comment)]
    /// Request and download the device certificate from Cumulocity
    ///
    /// - Generate a private key and Certificate Signing Request (CSR) for the device
    /// - Upload this CSR on Cumulocity, using the provided device identifier and security token
    /// - Loop till the device is registered by an administrator and the CSR accepted
    /// - Download and store the certificate created by Cumulocity
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

        #[clap(short = 'p', long = "one-time-password")]
        #[arg(
            env = "DEVICE_ONE_TIME_PASSWORD",
            hide_env_values = true,
            hide_default_value = true,
            default_value = ""
        )]
        /// The one-time password assigned to this device when registered to Cumulocity
        ///
        /// You will be prompted for input if the value is not provided or is empty
        one_time_password: String,

        #[clap(long)]
        /// The Cumulocity cloud profile (when the device is connected to several tenants)
        profile: Option<ProfileName>,

        /// Path to a Certificate Signing Request (CSR) ready to be used
        ///
        /// Providing the CSR is notably required when the request has to be signed
        /// by a tier tool owning the private key of the device.
        ///
        /// If none is provided a CSR is generated using the device id and private key
        /// configured for the given cloud profile.
        #[clap(long = "csr-path", global = true, value_hint = ValueHint::FilePath)]
        csr_path: Option<Utf8PathBuf>,

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

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_config::TEdgeConfigLocation;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[test_case(
        None,
        None,
        toml::toml!{
            [device]
            id = "test"
        },
        "test"
    )]
    #[test_case(
        None,
        Some(CloudArg::C8y{ profile: None }),
        toml::toml!{
            [device]
            id = "test"
        },
        "test"
    )]
    #[test_case(
        None,
        Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }),
        toml::toml!{
            [device]
            id = "test"
            [c8y.profiles.foo.device]
        },
        "test"
    )]
    #[test_case(
        None,
        Some(CloudArg::C8y{ profile: None }),
        toml::toml!{
            [device]
            id = "test"
            [c8y.device]
            id = "c8y-test"
            [c8y.profiles.foo.device]
            id = "c8y-foo-test"
        },
        "c8y-test"
    )]
    #[test_case(
        None,
        Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }),
        toml::toml!{
            [device]
            id = "test"
            [c8y.device]
            id = "c8y-test"
            [c8y.profiles.foo.device]
            id = "c8y-foo-test"
        },
        "c8y-foo-test"
    )]
    #[test_case(
        Some("input"),
        None,
        toml::toml!{
            [device]
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        None,
        toml::toml!{
            [device]
            id = "input"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        Some(CloudArg::C8y{ profile: None }),
        toml::toml!{
            [device]
            id = "test"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        Some(CloudArg::C8y{ profile: None }),
        toml::toml!{
            [c8y.device]
            id = "input"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }),
        toml::toml!{
            [c8y.profiles.foo.device]
            id = "input"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        None,
        toml::toml!{
            [device]
            id = "test"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        Some(CloudArg::C8y{ profile: None }),
        toml::toml!{
            [c8y.device]
            id = "c8y-test"
        },
        "input"
    )]
    #[test_case(
        Some("input"),
        Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }),
        toml::toml!{
            [c8y.profiles.foo.device]
            id = "c8y-foo-test"
        },
        "input"
    )]
    fn validate_get_device_id_returns_ok(
        input_id: Option<&str>,
        cloud_arg: Option<CloudArg>,
        toml: toml::Table,
        expected: &str,
    ) {
        let cloud: Option<Cloud> = cloud_arg.map(<_>::try_into).transpose().unwrap();
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_toml_content(toml);
        let location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let reader = location.load_sync().unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &reader, &cloud);
        assert_eq!(result.unwrap().as_str(), expected);
    }

    #[test_case(
        None,
        None,
        toml::toml!{
            [device]
        }
    )]
    fn validate_get_device_id_returns_err(
        input_id: Option<&str>,
        cloud_arg: Option<CloudArg>,
        toml: toml::Table,
    ) {
        let cloud: Option<Cloud> = cloud_arg.map(<_>::try_into).transpose().unwrap();
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_toml_content(toml);
        let location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let reader = location.load_sync().unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &reader, &cloud);
        assert!(result.is_err());
    }
}
