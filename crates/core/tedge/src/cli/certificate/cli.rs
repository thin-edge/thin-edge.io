use super::create::CreateCertCmd;
use super::create_csr::CreateCsrCmd;
use super::remove::RemoveCertCmd;
use super::renew::RenewCertCmd;
use super::show::ShowCertCmd;
use super::upload::*;

use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8PathBuf;
use clap::ValueHint;
use tedge_config::OptionalConfigError;
use tedge_config::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::WritableKey;

use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::BuildCommand;
use crate::command::BuildContext;
use crate::command::Command;
use crate::error::TEdgeError;
use crate::error::TEdgeError::MismatchedDeviceId;
use crate::ConfigError;

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
                    id: get_device_id(id, &config, &context.config_location, &cloud)?,
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    user: user.to_owned(),
                    group: group.to_owned(),
                    config_location: context.config_location,
                    writable_key: get_writable_key(&cloud)?,
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
                    id: get_device_id(id, &config, &context.config_location, &cloud)?,
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
                    // Use output file instead of csr_path from tedge config if provided
                    csr_path: output_path.unwrap_or_else(|| config.device.csr_path.clone()),
                    user: user.to_owned(),
                    group: group.to_owned(),
                    config_location: context.config_location,
                    writable_key: get_writable_key(&cloud)?,
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
                let cmd = UploadCertCmd {
                    device_id: c8y.device.id()?.clone(),
                    path: c8y.device.cert_path.clone(),
                    host: c8y.http.or_err()?.to_owned(),
                    cloud_root_certs: config.cloud_root_certs(),
                    username,
                    password,
                };
                cmd.into_boxed()
            }
            TEdgeCertCli::Renew { cloud } => {
                let cloud: Option<Cloud> = cloud.map(<_>::try_into).transpose()?;
                let cmd = RenewCertCmd {
                    cert_path: config.device_cert_path(cloud.as_ref())?.to_owned(),
                    key_path: config.device_key_path(cloud.as_ref())?.to_owned(),
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
/// If the provided device ID mismatches the one from the config, it returns an error
fn get_device_id(
    id: Option<String>,
    config: &TEdgeConfig,
    config_location: &TEdgeConfigLocation,
    cloud: &Option<Cloud>,
) -> Result<String, TEdgeError> {
    let config_id = config.device_id(cloud.as_ref()).ok();
    let writable_key = get_writable_key(cloud)?;
    let dto = config_location
        .load_dto_from_toml_and_env()
        .context("failed to read tedge.toml file")?;

    match id {
        None => match config_id {
            None => Err(anyhow!("No device ID is provided. Use `--device-id <name>` option to specify the device ID.").into()),
            Some(config_id) => Ok(config_id.into()),
        }
        Some(input_id) if config_id.is_none() => Ok(input_id),
        Some(input_id) if input_id == config_id.unwrap() => Ok(input_id),
        Some(input_id) => match cloud.as_ref().map(<_>::into) {
            None => Err(MismatchedDeviceId {
                input_id,
                config_id: config_id.unwrap().into(),
                writable_key,
            }),
            Some(tedge_config::Cloud::C8y(profile)) => {
                let key = profile.map(|name| name.to_string());
                let c8y_dto = dto.c8y.try_get(key.as_deref(), "c8y")?;
                if c8y_dto.device.id.is_some() {
                    Err(MismatchedDeviceId {
                        input_id,
                        config_id: config_id.unwrap().into(),
                        writable_key,
                    })
                } else {
                    Ok(input_id)
                }
            }
            Some(tedge_config::Cloud::Az(profile)) => {
                let key = profile.map(|name| name.to_string());
                let az_dto = dto.az.try_get(key.as_deref(), "az")?;
                if az_dto.device.id.is_some() {
                    Err(MismatchedDeviceId {
                        input_id,
                        config_id: config_id.unwrap().into(),
                        writable_key,
                    })
                } else {
                    Ok(input_id)
                }
            }
            Some(tedge_config::Cloud::Aws(profile)) => {
                let key = profile.map(|name| name.to_string());
                let aws_dto = dto.c8y.try_get(key.as_deref(), "aws")?;
                if aws_dto.device.id.is_some() {
                    Err(MismatchedDeviceId {
                        input_id,
                        config_id: config_id.unwrap().into(),
                        writable_key,
                    })
                } else {
                    Ok(input_id)
                }
            }
        },
    }
}

pub(crate) fn get_writable_key(cloud: &Option<Cloud>) -> Result<WritableKey, anyhow::Error> {
    let key = match cloud {
        None => WritableKey::DeviceId,
        Some(cloud) => {
            let key = match cloud {
                Cloud::C8y(_) => WritableKey::C8yDeviceId(None),
                Cloud::Azure(_) => WritableKey::AzDeviceId(None),
                Cloud::Aws(_) => WritableKey::AwsDeviceId(None),
            };
            let profile = cloud.profile_name().cloned();
            crate::try_with_profile!(key, profile)
        }
    };
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[test_case("test", None, None, toml::toml!{
    [device]
        id = "test"
    })]
    #[test_case("test", Some("test"), None, toml::toml!{
    [device]
    })]
    #[test_case("c8y-test", None, Some(CloudArg::C8y{ profile: None }), toml::toml!{
    [c8y.device]
        id = "c8y-test"
    })]
    #[test_case("c8y-test", Some("c8y-test"), Some(CloudArg::C8y{ profile: None }), toml::toml!{
    [c8y.device]
    })]
    #[test_case("c8y-test", Some("c8y-test"), Some(CloudArg::C8y{ profile: None }), toml::toml!{
    [device]
        id = "test"
    })]
    #[test_case("c8y-foo-test", Some("c8y-foo-test"), Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }), toml::toml!{
    [device]
        id = "test"
    [c8y.device]
        id = "c8y-test"
    })]
    #[test_case("c8y-foo-test", None, Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }), toml::toml!{
    [device]
        id = "test"
    [c8y.device]
        id = "c8y-test"
    [c8y.profiles.foo.device]
        id = "c8y-foo-test"
    })]
    fn validate_get_device_id_returns_ok(
        expected: &str,
        input_id: Option<&str>,
        cloud_arg: Option<CloudArg>,
        toml: toml::Table,
    ) {
        let cloud: Option<Cloud> = cloud_arg.map(<_>::try_into).transpose().unwrap();
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_toml_content(toml);
        let location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let reader = location.load().unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &reader, &location, &cloud);
        assert_eq!(result.unwrap().as_str(), expected);
    }

    #[test_case(None, None, toml::toml!{
    [device]
    })]
    #[test_case(Some("input"), None, toml::toml!{
    [device]
        id = "test"
    })]
    #[test_case(Some("input"), Some(CloudArg::C8y{ profile: None }), toml::toml!{
    [c8y.device]
        id = "c8y-test"
    })]
    #[test_case(Some("input"), Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap()) }), toml::toml!{
    [c8y.profiles.foo.device]
        id = "c8y-foo-test"
    })]
    fn validate_get_device_id_returns_err(
        input_id: Option<&str>,
        cloud_arg: Option<CloudArg>,
        toml: toml::Table,
    ) {
        let cloud: Option<Cloud> = cloud_arg.map(<_>::try_into).transpose().unwrap();
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_toml_content(toml);
        let location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let reader = location.load().unwrap();
        let id = input_id.map(|s| s.to_string());
        let result = get_device_id(id, &reader, &location, &cloud);
        dbg!(&result);
        assert!(result.is_err());
    }

    #[test_case(None, WritableKey::DeviceId)]
    #[test_case(Some(CloudArg::C8y{ profile: None }), WritableKey::C8yDeviceId(None))]
    #[test_case(Some(CloudArg::Az{ profile: None }), WritableKey::AzDeviceId(None))]
    #[test_case(Some(CloudArg::Aws{ profile: None }), WritableKey::AwsDeviceId(None))]
    #[test_case(Some(CloudArg::C8y{ profile: Some("foo".parse().unwrap())}), WritableKey::C8yDeviceId(Some("foo".into())))]
    #[test_case(Some(CloudArg::Az{ profile: Some("foo".parse().unwrap())}), WritableKey::AzDeviceId(Some("foo".into())))]
    #[test_case(Some(CloudArg::Aws{ profile: Some("foo".parse().unwrap())}), WritableKey::AwsDeviceId(Some("foo".into())))]
    fn validate_get_writable_key(cloud_arg: Option<CloudArg>, expected: WritableKey) {
        let cloud: Option<Cloud> = cloud_arg.map(<_>::try_into).transpose().unwrap();
        let key = get_writable_key(&cloud).unwrap();
        assert_eq!(key, expected);
    }
}
