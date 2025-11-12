use super::create::CreateCertCmd;
use super::create_csr::CreateCsrCmd;
use super::remove::RemoveCertCmd;
use super::renew::RenewCertCmd;
use super::show::ShowCertCmd;
use crate::certificate_is_self_signed;
use crate::cli::certificate::c8y;
use crate::cli::certificate::create_csr::Key;
use crate::cli::certificate::create_key::CreateKeyHsmCmd;
use crate::cli::certificate::create_key::EcCurve;
use crate::cli::certificate::create_key::KeyType;
use crate::cli::certificate::create_key::RsaBits;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::CertificateShift;
use crate::ConfigError;
use anyhow::anyhow;
use anyhow::Context;
use c8y_api::http_proxy::C8yEndPoint;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use clap::ValueHint;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tracing::debug;

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

    /// Generate a new keypair on the PKCS #11 token and select it to be used.
    ///
    /// Can be used to generate a keypair on the TOKEN. If TOKEN argument is not provided, the
    /// command prints the available tokens.
    ///
    /// If TOKEN is provided, the command generates an RSA or an ECDSA keypair on the token. When
    /// using RSA, `--bits` is used to set the size of the key, when using ECDSA, `--curve` is used.
    ///
    /// After the key is generated, tedge config is updated to use the new key using
    /// `device.key_uri` property. Depending on the selected cloud, we use `device.key_uri` setting
    /// for that cloud, e.g. `create-key-hsm c8y` will write to `c8y.device.key_uri`.
    CreateKeyHsm {
        /// Human readable description (CKA_LABEL attribute) for the key.
        #[arg(long, default_value = "tedge")]
        label: String,

        /// Key identifier for the keypair (CKA_ID attribute).
        ///
        /// If provided and no object exists on the token with the same ID, this will be the ID of
        /// the new keypair. If an object with this ID already exists, the operation will return an
        /// error. If not provided, a random ID will be generated and used by the keypair.
        ///
        /// The id shall be provided as a sequence of hex digits without `0x` prefix, optionally
        /// separated by spaces, e.g. `--id 010203` or `--id "01 02 03"`.
        #[arg(long)]
        id: Option<String>,

        /// The type of the key.
        #[arg(long, default_value = "ecdsa")]
        r#type: KeyType,

        /// The size of the RSA keys in bits. Should only be used with --type rsa.
        #[arg(long, default_value = "2048", group = "key_params")]
        bits: RsaBits,

        /// The curve (size) of the ECDSA key. Should only be used with --type ecdsa.
        #[arg(long, default_value = "p256", group = "key_params")]
        curve: EcCurve,

        /// User PIN value for logging into the PKCS #11 token.
        ///
        /// This flag can be used to provide a PIN when creating a new key without needing to update
        /// tedge-config, which can be helpful when initializing keys on new tokens.
        ///
        /// Note that in contrast to the URI of the key, which will be written to tedge-config
        /// automatically when the keypair is created, PIN will not be written automatically and may
        /// be needed to written manually using tedge config set (if not using tedge-p11-server with
        /// the correct default PIN).
        #[arg(long)]
        pin: Option<String>,

        /// Path where public key will be saved when a keypair is generated.
        #[arg(long)]
        outfile_pubkey: Option<Box<Utf8Path>>,

        // can't document subcommands here because one would have to document variants of the enum
        // but this type is used in other places
        #[clap(subcommand)]
        cloud: Option<CloudArg>,

        /// The URI of the token where the keypair should be created.
        ///
        /// If this argument is missing, a list of available initialized tokens will be shown. The
        /// token needs to be initialized to be able to generate keys.
        token: Option<String>,
    },

    /// Renew the device certificate
    ///
    /// The current certificate is left unchanged and a new certificate file is created,
    /// of which path is derived from the current certificate path by adding a `.new` suffix.
    ///
    /// The device certificate will be replaced by the new certificate, after proper validation,
    /// by the `tedge connect` command.
    ///
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

        /// Certificate Authority (CA) used to renew the certificate
        ///
        /// Cumulocity CA is currently the only supported one and this is the default,
        /// even if the current certificate has not been signed by Cumulocity.
        /// In most cases, the default behavior is what you want:
        /// substitute a proper CA-signed certificate for a self-signed certificate.
        ///
        /// However, if this is not the case, or if the cloud endpoint doesn't provide a CA:
        /// use `--ca self-signed` to get a renewed self-signed certificate.
        #[clap(long = "ca", default_value_t = CA::C8y, global = true)]
        ca: CA,

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

        /// Show the new certificate, if any, instead of the current one
        #[clap(long = "new", default_value_t = false, global = true)]
        show_new: bool,

        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Remove the device certificate
    Remove {
        #[clap(subcommand)]
        cloud: Option<CloudArg>,
    },

    /// Upload the device certificate to the cloud
    ///
    /// If the device certificate has been renewed,
    /// then the new certificate is uploaded.
    #[clap(subcommand)]
    Upload(UploadCertCli),

    /// Request and download the device certificate
    #[clap(subcommand)]
    Download(DownloadCertCli),
}

#[derive(clap::ValueEnum, Clone, Debug, Eq, PartialEq, strum_macros::Display)]
pub enum CA {
    #[strum(serialize = "self-signed")]
    SelfSigned,

    #[strum(serialize = "c8y")]
    C8y,
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeCertCli {
    async fn build_command(self, config: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
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
                    id: get_device_id(id, config, &cloud)?,
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
                debug!(?cloud);
                let cloud_config = match cloud.as_ref() {
                    Some(c) => Some(config.as_cloud_config(c.into()).await?),
                    None => None,
                };
                let cryptoki = config
                    .device
                    .cryptoki_config(cloud_config.as_ref().map(|c| c as &dyn CloudConfig))?;
                let key = cryptoki
                    .map(super::create_csr::Key::Cryptoki)
                    .unwrap_or(Key::Local(
                        config.device_key_path(cloud.as_ref())?.to_owned(),
                    ));
                debug!(?key);
                let current_cert = config
                    .device_cert_path(cloud.as_ref())
                    .map(|c| c.to_owned())
                    .ok();
                debug!(?current_cert);

                let cmd = CreateCsrCmd {
                    id: get_device_id(id, config, &cloud)?,
                    key,
                    // Use output file instead of csr_path from tedge config if provided
                    csr_path: if let Some(output_path) = output_path {
                        output_path
                    } else {
                        config.device_csr_path(cloud.as_ref())?.to_owned()
                    },
                    current_cert,
                    user: user.to_owned(),
                    group: group.to_owned(),
                    csr_template,
                };
                cmd.into_boxed()
            }

            TEdgeCertCli::CreateKeyHsm {
                bits,
                label,
                r#type,
                curve,
                id,
                pin,
                outfile_pubkey,

                cloud,
                token,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cloud_config = match cloud.as_ref() {
                    Some(c) => Some(config.as_cloud_config(c.into()).await?),
                    None => None,
                };
                let cryptoki_config = config
                    .device
                    .cryptoki_config(cloud_config.as_ref().map(|c| c as &dyn CloudConfig))?
                    .context("Cryptoki config is not enabled")?;

                CreateKeyHsmCmd {
                    cryptoki_config,
                    label,
                    r#type,
                    bits,
                    curve,
                    id,
                    pin,
                    outfile_pubkey,
                    cloud,
                    token,
                }
                .into_boxed()
            }
            TEdgeCertCli::Show {
                cloud,
                cert_path,
                show_new,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let device_cert_path = config.device_cert_path(cloud.as_ref())?.to_owned();
                let cert_path = cert_path.unwrap_or(device_cert_path);
                let cmd = ShowCertCmd {
                    cert_path: if show_new {
                        CertificateShift::new_certificate_path(&cert_path)
                    } else {
                        cert_path
                    },
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
                let c8y = config
                    .mapper_config::<C8yMapperSpecificConfig>(&profile)
                    .await?;
                let cmd = c8y::UploadCertCmd {
                    device_id: c8y.device.id()?.clone(),
                    path: c8y.device.cert_path.clone().into(),
                    host: c8y.cloud_specific.http.to_owned(),
                    cloud_root_certs: config.cloud_root_certs()?,
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
                url,
                retry_every,
                max_timeout,
            }) => {
                let c8y_config = config
                    .mapper_config::<C8yMapperSpecificConfig>(&profile)
                    .await?;

                let (csr_path, generate_csr) = match csr_path {
                    None => (c8y_config.device.csr_path.clone().into(), true),
                    Some(csr_path) => (csr_path, false),
                };

                let c8y_url = match url {
                    Some(v) => v,
                    None => c8y_config.cloud_specific.http.to_owned(),
                };

                let cryptoki = config.device.cryptoki_config(Some(&*c8y_config))?;
                let key = cryptoki
                    .map(super::create_csr::Key::Cryptoki)
                    .unwrap_or(Key::Local(
                        config
                            .device_key_path(Some(tedge_config::tedge_toml::Cloud::C8y(
                                profile.as_ref(),
                            )))?
                            .to_owned(),
                    ));
                let cmd = c8y::DownloadCertCmd {
                    device_id: id,
                    one_time_password: token,
                    c8y_url,
                    root_certs: config.cloud_root_certs()?,
                    cert_path: c8y_config.device.cert_path.to_owned().into(),
                    key,
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
                ca,
            } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cert_path = config.device_cert_path(cloud.as_ref())?.to_owned();
                let key_path = config.device_key_path(cloud.as_ref())?.to_owned();
                let new_cert_path = CertificateShift::new_certificate_path(&cert_path);

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

                if is_self_signed && ca == CA::SelfSigned {
                    let cmd = RenewCertCmd {
                        cert_path,
                        new_cert_path,
                        key_path,
                        csr_template,
                    };
                    cmd.into_boxed()
                } else if ca == CA::SelfSigned {
                    return Err(
                        anyhow!("Cannot renew certificate with self-signed ca: {cert_path} is not self-signed").into()
                    );
                } else {
                    let (csr_path, generate_csr) = match csr_path {
                        None => (config.device_csr_path(cloud.as_ref())?.to_owned(), true),
                        Some(csr_path) => (csr_path, false),
                    };
                    let c8y = match &cloud {
                        None => {
                            let c8y_config = config.mapper_config(&None::<ProfileName>).await?;
                            C8yEndPoint::local_proxy(&c8y_config)?
                        }
                        #[cfg(feature = "c8y")]
                        Some(Cloud::C8y(profile)) => {
                            let c8y_config = config.mapper_config(profile).await?;
                            C8yEndPoint::local_proxy(&c8y_config)?
                        }
                        #[cfg(any(feature = "aws", feature = "azure"))]
                        Some(cloud) => {
                            return Err(
                                anyhow!("Certificate renewal is not supported for {cloud}").into()
                            )
                        }
                    };

                    let cloud_config = match cloud.as_ref() {
                        Some(c) => Some(config.as_cloud_config(c.into()).await?),
                        None => None,
                    };
                    let cryptoki = config
                        .device
                        .cryptoki_config(cloud_config.as_ref().map(|c| c as &dyn CloudConfig))?;
                    let key = cryptoki
                        .map(super::create_csr::Key::Cryptoki)
                        .unwrap_or(Key::Local(
                            config.device_key_path(cloud.as_ref())?.to_owned(),
                        ));
                    let cmd = c8y::RenewCertCmd {
                        c8y,
                        http_config: config.cloud_root_certs()?,
                        identity: config.http.client.auth.identity()?,
                        cert_path,
                        new_cert_path,
                        key,
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

        #[clap(long = "password", allow_hyphen_values = true)]
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

        #[clap(short = 'p', long = "one-time-password", allow_hyphen_values = true)]
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

        /// URL to download the certificate from
        /// If not provided, the c8y.http value from the configuration will be used.
        /// Example: example.eu-latest.cumulocity.com
        #[clap(long, value_hint = ValueHint::Url)]
        url: Option<HostPort<HTTPS_PORT>>,

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
        let config = TEdgeConfig::load_sync(ttd.path()).unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &config, &cloud);
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
        let config = TEdgeConfig::load_sync(ttd.path()).unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &config, &cloud);
        assert!(result.is_err());
    }
}
