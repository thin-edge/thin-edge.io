use crate::cli::common::mapper_name_completions;
use crate::cli::common::profile_completions;
use crate::cli::common::ConnectCloudArg;
use crate::cli::connect::*;
use crate::command::BuildCommand;
use crate::command::Command;
use clap_complete::ArgValueCandidates;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_system_services::service_manager;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeConnectOpt {
    /// Test an existing connection
    #[clap(long = "test")]
    is_test_connection: bool,

    /// Ignore connection registration and connection check
    #[clap(long = "offline")]
    offline_mode: bool,

    /// The cloud or custom mapper to connect to (e.g. c8y, aws, az, or a custom mapper name)
    #[arg(add(ArgValueCandidates::new(mapper_name_completions)))]
    cloud: String,

    /// The cloud profile to use
    ///
    /// [env: TEDGE_CLOUD_PROFILE]
    #[clap(long)]
    #[arg(add(ArgValueCandidates::new(profile_completions)))]
    profile: Option<ProfileName>,
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeConnectOpt {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        let Self {
            is_test_connection,
            offline_mode,
            cloud,
            profile,
        } = self;
        let cloud = ConnectCloudArg {
            name: cloud,
            profile,
        }
        .into_cloud();
        Ok(Box::new(ConnectCommand {
            service_manager: service_manager(config.root_dir())?,
            cloud,
            is_test_connection,
            offline_mode,
            is_reconnect: false,
        }))
    }
}
