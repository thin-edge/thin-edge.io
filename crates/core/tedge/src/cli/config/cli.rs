use crate::cli::common::profile_completions;
use crate::cli::config::commands::*;
use crate::cli::config::keys::FederatedReadableKey;
use crate::cli::config::keys::FederatedWritableKey;
use crate::command::*;
use crate::ConfigError;
use clap_complete::ArgValueCandidates;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

#[derive(clap::Subcommand, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(FederatedReadableKey::completions))]
        key: FederatedReadableKey,

        /// The cloud profile you wish to use, if accessing a cloud configuration
        /// (i.e. `c8y.*`, `az.*` or `aws.*`). If you don't wish to use cloud profiles,
        /// or want to access the default profile, don't supply this.
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add = ArgValueCandidates::new(profile_completions))]
        profile: Option<ProfileName>,
    },

    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(FederatedWritableKey::completions))]
        key: FederatedWritableKey,

        /// Configuration value.
        value: String,

        /// The cloud profile you wish to use, if accessing a cloud configuration
        /// (i.e. `c8y.*`, `az.*` or `aws.*`). If you don't wish to use cloud profiles,
        /// or want to access the default profile, don't supply this.
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add = ArgValueCandidates::new(profile_completions))]
        profile: Option<ProfileName>,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(FederatedWritableKey::completions))]
        key: FederatedWritableKey,

        /// The cloud profile you wish to use, if accessing a cloud configuration
        /// (i.e. `c8y.*`, `az.*` or `aws.*`). If you don't wish to use cloud profiles,
        /// or want to access the default profile, don't supply this.
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add = ArgValueCandidates::new(profile_completions))]
        profile: Option<ProfileName>,
    },

    /// Append or set the provided configuration key with the given value
    Add {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(FederatedWritableKey::completions))]
        key: FederatedWritableKey,

        /// Configuration value.
        value: String,

        /// The cloud profile you wish to use, if accessing a cloud configuration
        /// (i.e. `c8y.*`, `az.*` or `aws.*`). If you don't wish to use cloud profiles,
        /// or want to access the default profile, don't supply this.
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add = ArgValueCandidates::new(profile_completions))]
        profile: Option<ProfileName>,
    },

    /// Remove value from the provided configuration key
    Remove {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(FederatedWritableKey::completions))]
        key: FederatedWritableKey,

        /// Configuration value.
        value: String,

        /// The cloud profile you wish to use, if accessing a cloud configuration
        /// (i.e. `c8y.*`, `az.*` or `aws.*`). If you don't wish to use cloud profiles,
        /// or want to access the default profile, don't supply this.
        ///
        /// [env: TEDGE_CLOUD_PROFILE]
        #[clap(long)]
        #[arg(add = ArgValueCandidates::new(profile_completions))]
        profile: Option<ProfileName>,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[clap(long = "all")]
        is_all: bool,

        /// Prints all keys and descriptions with example values
        #[clap(long = "doc")]
        is_doc: bool,

        /// Prints only the keys that contain the provided filter string
        filter: Option<String>,
    },

    #[clap(hide = true)]
    Upgrade,
}

#[macro_export]
macro_rules! try_with_profile {
    ($key:ident, $profile:ident) => {{
        use anyhow::Context;
        let profiled_key = match $profile {
            None => $key,
            Some(profile) => $key.try_with_profile(profile)?,
        };
        match std::env::var("TEDGE_CLOUD_PROFILE").as_deref() {
            Ok("") | Err(_) => profiled_key,
            Ok(value) => profiled_key
                .clone()
                .try_with_profile(value.parse().context("Parsing TEDGE_CLOUD_PROFILE")?)
                .unwrap_or(profiled_key),
        }
    }};
}

#[cfg(feature = "mapper-config")]
fn reject_profile_for_mapper_key(
    key: &str,
    profile: &Option<ProfileName>,
) -> Result<(), ConfigError> {
    if profile.is_some() {
        return Err(anyhow::anyhow!("--profile is not supported for mapper key '{key}'").into());
    }
    Ok(())
}

#[async_trait::async_trait]
impl BuildCommand for ConfigCmd {
    async fn build_command(self, _: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            ConfigCmd::Get { key, profile } => match key {
                FederatedReadableKey::Core(key) => Ok(GetConfigCommand {
                    key: try_with_profile!(key, profile),
                }
                .into_boxed()),
                #[cfg(feature = "mapper-config")]
                FederatedReadableKey::Mapper(key) => {
                    reject_profile_for_mapper_key(&key, &profile)?;
                    Ok(MapperGetConfigCommand { key }.into_boxed())
                }
            },
            ConfigCmd::Set {
                key,
                value,
                profile,
            } => match key {
                FederatedWritableKey::Core(key) => Ok(SetConfigCommand {
                    key: try_with_profile!(key, profile),
                    value,
                }
                .into_boxed()),
                #[cfg(feature = "mapper-config")]
                FederatedWritableKey::Mapper(key) => {
                    reject_profile_for_mapper_key(&key, &profile)?;
                    Ok(MapperMutateConfigCommand::set(key, value).into_boxed())
                }
            },
            ConfigCmd::Unset { key, profile } => match key {
                FederatedWritableKey::Core(key) => Ok(UnsetConfigCommand {
                    key: try_with_profile!(key, profile),
                }
                .into_boxed()),
                #[cfg(feature = "mapper-config")]
                FederatedWritableKey::Mapper(key) => {
                    reject_profile_for_mapper_key(&key, &profile)?;
                    Ok(MapperMutateConfigCommand::unset(key).into_boxed())
                }
            },
            ConfigCmd::Add {
                key,
                value,
                profile,
            } => match key {
                FederatedWritableKey::Core(key) => Ok(AddConfigCommand {
                    key: try_with_profile!(key, profile),
                    value,
                }
                .into_boxed()),
                #[cfg(feature = "mapper-config")]
                FederatedWritableKey::Mapper(key) => {
                    reject_profile_for_mapper_key(&key, &profile)?;
                    Ok(MapperMutateConfigCommand::add(key, value).into_boxed())
                }
            },
            ConfigCmd::Remove {
                key,
                value,
                profile,
            } => match key {
                FederatedWritableKey::Core(key) => Ok(RemoveConfigCommand {
                    key: try_with_profile!(key, profile),
                    value,
                }
                .into_boxed()),
                #[cfg(feature = "mapper-config")]
                FederatedWritableKey::Mapper(key) => {
                    reject_profile_for_mapper_key(&key, &profile)?;
                    Ok(MapperMutateConfigCommand::remove(key, value).into_boxed())
                }
            },
            ConfigCmd::List {
                is_all,
                is_doc,
                filter,
            } => Ok(ListConfigCommand {
                is_all,
                is_doc,
                filter,
            }
            .into_boxed()),
            ConfigCmd::Upgrade => Ok(UpgradeConfigCommand.into_boxed()),
        }
    }
}
