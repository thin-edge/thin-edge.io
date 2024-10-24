use crate::aws::mapper::AwsMapper;
use crate::az::mapper::AzureMapper;
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use anyhow::Context;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use std::fmt;
use tedge_config::get_config_dir;
use tedge_config::system_services::log_init;
use tedge_config::system_services::LogConfigArgs;
use tedge_config::PathBuf;
use tedge_config::ProfileName;
use tracing::log::warn;

mod aws;
mod az;
mod c8y;
mod collectd;
mod core;

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

fn lookup_component(
    component_name: &MapperName,
    profile: Option<ProfileName>,
) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az => Box::new(AzureMapper {
            profile: read_and_set_var!(profile, "AZ_PROFILE"),
        }),
        MapperName::Aws => Box::new(AwsMapper {
            profile: read_and_set_var!(profile, "AWS_PROFILE"),
        }),
        MapperName::Collectd => Box::new(CollectdMapper),
        MapperName::C8y => Box::new(CumulocityMapper {
            profile: read_and_set_var!(profile, "C8Y_PROFILE"),
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
    pub name: MapperName,

    #[command(flatten)]
    pub log_args: LogConfigArgs,

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    pub init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[clap(short, long)]
    pub clear: bool,

    /// Start the mapper from custom path
    ///
    /// [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
    #[clap(
        long = "config-dir",
        default_value = get_config_dir().into_os_string(),
        hide_env_values = true,
        hide_default_value = true,
    )]
    pub config_dir: PathBuf,

    #[clap(long, global = true, hide = true)]
    pub profile: Option<ProfileName>,
}

#[derive(Debug, clap::Subcommand)]
pub enum MapperName {
    Az,
    Aws,
    C8y,
    Collectd,
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az => write!(f, "tedge-mapper-az"),
            MapperName::Aws => write!(f, "tedge-mapper-aws"),
            MapperName::C8y => write!(f, "tedge-mapper-c8y"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
        }
    }
}

pub async fn run(mapper_opt: MapperOpt) -> anyhow::Result<()> {
    let component = lookup_component(&mapper_opt.name, mapper_opt.profile.clone());

    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&mapper_opt.config_dir);
    let config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

    log_init(
        "tedge-mapper",
        &mapper_opt.log_args,
        &tedge_config_location.tedge_config_root_path,
    )?;

    // Run only one instance of a mapper (if enabled)
    let mut _flock = None;
    if config.run.lock_files {
        let run_dir = config.run.path.as_std_path();
        _flock = check_another_instance_is_not_running(&mapper_opt.name.to_string(), run_dir)?;
    }

    if mapper_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else if mapper_opt.clear {
        warn!("This --clear option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        component
            .start(config, mapper_opt.config_dir.as_ref())
            .await
    }
}
