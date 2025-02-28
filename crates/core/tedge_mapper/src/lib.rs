use std::fmt;

use crate::aws::mapper::AwsMapper;
use crate::az::mapper::AzureMapper;
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use anyhow::Context;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use tedge_config::cli::CommonArgs;
use tedge_config::system_services::log_init;
use tedge_config::ProfileName;
use tracing::log::warn;

mod aws;
mod az;
mod c8y;
mod collectd;
mod core;

/// Set the cloud profile either from the CLI argument or env variable,
/// then set the environment variable so child processes automatically
/// have the correct profile set.
macro_rules! read_and_set_var {
    ($profile:ident, $var:literal) => {
        $profile
            .or_else(|| {
                Some(
                    std::env::var($var)
                        .ok()?
                        .parse()
                        .context(concat!("Reading environment variable ", $var))
                        .unwrap(),
                )
            })
            .inspect(|profile| std::env::set_var($var, profile))
    };
}

fn lookup_component(component_name: MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az { profile } => Box::new(AzureMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        MapperName::Aws { profile } => Box::new(AwsMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
        MapperName::Collectd => Box::new(CollectdMapper),
        MapperName::C8y { profile } => Box::new(CumulocityMapper {
            profile: read_and_set_var!(profile, "TEDGE_CLOUD_PROFILE"),
        }),
    }
}

#[derive(Debug, Parser)]
#[clap(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct MapperOpt {
    #[clap(subcommand)]
    name: MapperName,

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[clap(short, long)]
    clear: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, clap::Subcommand, Clone)]
#[clap(rename_all = "snake_case")]
pub enum MapperName {
    Az {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    Aws {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    C8y {
        /// The cloud profile to use
        #[clap(long)]
        profile: Option<ProfileName>,
    },
    Collectd,
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az { profile: None } => write!(f, "tedge-mapper-az"),
            MapperName::Az {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-az@{profile}"),
            MapperName::Aws { profile: None } => write!(f, "tedge-mapper-aws"),
            MapperName::Aws {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-aws@{profile}"),
            MapperName::C8y { profile: None } => write!(f, "tedge-mapper-c8y"),
            MapperName::C8y {
                profile: Some(profile),
            } => write!(f, "tedge-mapper-c8y@{profile}"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
        }
    }
}

pub async fn run(mapper_opt: MapperOpt) -> anyhow::Result<()> {
    let mapper_name = mapper_opt.name.to_string();
    let component = lookup_component(mapper_opt.name);

    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&mapper_opt.common.config_dir);
    let config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

    let _guard = log_init(
        "tedge-mapper",
        &mapper_opt.common.log_args,
        &tedge_config_location.tedge_config_root_path,
    )?;

    // Run only one instance of a mapper (if enabled)
    let mut _flock = None;
    if config.run.lock_files {
        let run_dir = config.run.path.as_std_path();
        _flock = check_another_instance_is_not_running(&mapper_name, run_dir)?;
    }

    if mapper_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else if mapper_opt.clear {
        warn!("This --clear option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        component
            .start(config, mapper_opt.common.config_dir.as_ref())
            .await
    }
}
