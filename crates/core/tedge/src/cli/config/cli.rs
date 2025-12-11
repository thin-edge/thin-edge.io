use crate::cli::common::profile_completions;
use crate::cli::config::commands::*;
use crate::command::*;
use crate::ConfigError;
use clap_complete::ArgValueCandidates;
use tedge_config::models::CloudType;
use tedge_config::tedge_toml::mapper_config::compat::IsCloudConfig;
use tedge_config::tedge_toml::mapper_config::AwsMapperSpecificConfig;
use tedge_config::tedge_toml::mapper_config::AzMapperSpecificConfig;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;

#[derive(clap::Subcommand, Debug)]
pub enum ConfigCmd {
    /// Get the value of the provided configuration key
    Get {
        /// Configuration key. Run `tedge config list --doc` for available keys
        #[arg(add = ArgValueCandidates::new(ReadableKey::completions))]
        key: ReadableKey,

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
        #[arg(add = ArgValueCandidates::new(WritableKey::completions))]
        key: WritableKey,

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
        #[arg(add = ArgValueCandidates::new(WritableKey::completions))]
        key: WritableKey,

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
        #[arg(add = ArgValueCandidates::new(WritableKey::completions))]
        key: WritableKey,

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
        #[arg(add = ArgValueCandidates::new(WritableKey::completions))]
        key: WritableKey,

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

#[async_trait::async_trait]
impl BuildCommand for ConfigCmd {
    async fn build_command(self, _: &TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            ConfigCmd::Get { key, profile } => Ok(GetConfigCommand {
                key: try_with_profile!(key, profile),
            }
            .into_boxed()),
            ConfigCmd::Set {
                key,
                value,
                profile,
            } => Ok(SetConfigCommand {
                key: try_with_profile!(key, profile),
                value,
            }
            .into_boxed()),
            ConfigCmd::Unset { key, profile } => Ok(UnsetConfigCommand {
                key: try_with_profile!(key, profile),
            }
            .into_boxed()),
            ConfigCmd::Add {
                key,
                value,
                profile,
            } => Ok(AddConfigCommand {
                key: try_with_profile!(key, profile),
                value,
            }
            .into_boxed()),
            ConfigCmd::Remove {
                key,
                value,
                profile,
            } => Ok(RemoveConfigCommand {
                key: try_with_profile!(key, profile),
                value,
            }
            .into_boxed()),
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
        }
    }
}

pub async fn restrict_cloud_config_access(
    cmd: &str,
    key: &(impl IsCloudConfig + std::fmt::Display),
    tedge_config: &TEdgeConfig,
) -> anyhow::Result<()> {
    use tedge_config::ConfigDecision as CD;
    let Some((cloud, profile)) = key.cloud_type_for() else {
        // Not a cloud config, we don't need to worry about
        return Ok(());
    };
    let (cloud, config_source) = match cloud {
        CloudType::C8y => {
            let source = tedge_config
                .decide_config_source::<C8yMapperSpecificConfig>(profile.as_ref())
                .await;
            (cloud, source)
        }
        CloudType::Az => {
            let source = tedge_config
                .decide_config_source::<AzMapperSpecificConfig>(profile.as_ref())
                .await;
            (cloud, source)
        }
        CloudType::Aws => {
            let source = tedge_config
                .decide_config_source::<AwsMapperSpecificConfig>(profile.as_ref())
                .await;
            (cloud, source)
        }
    };

    match config_source {
            // Cloud config stored in tedge.toml
            CD::LoadLegacy => Ok(()),

            // The cloud config has been migrated to new format
            CD::LoadNew { path } | CD::NotFound { path } => {
                Err(anyhow::anyhow!("`tedge config {cmd}` cannot be used to update {cloud} mapper config. Please directly edit {path} to update {key}"))
            },

            // Mappers directory exists, but we can't see inside it to check if the cloud is migrated
            CD::PermissionError { mapper_config_dir, error } => {
                let message = format!("Could not access {mapper_config_dir} to establish whether {cloud} mapper config is stored in `tedge.toml` or a separate file");
                let error = anyhow::Error::new(error);
                Err(error.context(message))
            }
        }
}
