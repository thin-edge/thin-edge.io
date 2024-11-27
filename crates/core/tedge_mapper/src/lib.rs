use crate::aws::mapper::AwsMapper;
use crate::az::mapper::AzureMapper;
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use anyhow::bail;
use anyhow::Context;
use clap::Command;
use clap::FromArgMatches;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use std::fmt;
use std::str::FromStr;
use tedge_config::cli::CommonArgs;
use tedge_config::system_services::log_init;
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

fn lookup_component(component_name: MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az(profile) => Box::new(AzureMapper {
            profile: read_and_set_var!(profile, "AZ_PROFILE"),
        }),
        MapperName::Aws(profile) => Box::new(AwsMapper {
            profile: read_and_set_var!(profile, "AWS_PROFILE"),
        }),
        MapperName::Collectd => Box::new(CollectdMapper),
        MapperName::C8y(profile) => Box::new(CumulocityMapper {
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

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    pub init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[clap(short, long)]
    pub clear: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, Clone)]
pub enum MapperName {
    Az(Option<ProfileName>),
    Aws(Option<ProfileName>),
    C8y(Option<ProfileName>),
    Collectd,
}

impl FromStr for MapperName {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match (s, s.split_once("@")) {
            ("az", _) => Ok(Self::Az(None)),
            ("aws", _) => Ok(Self::Aws(None)),
            ("c8y", _) => Ok(Self::C8y(None)),
            ("collectd", _) => Ok(Self::Collectd),
            (_, Some(("az", profile))) => Ok(Self::Az(Some(profile.parse()?))),
            (_, Some(("aws", profile))) => Ok(Self::Aws(Some(profile.parse()?))),
            (_, Some(("c8y", profile))) => Ok(Self::C8y(Some(profile.parse()?))),
            _ => bail!("Unknown subcommand `{s}`"),
        }
    }
}

impl FromArgMatches for MapperName {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        match matches.subcommand() {
            Some((cmd, _)) => cmd.parse().map_err(|_| {
                clap::Error::raw(
                    clap::error::ErrorKind::InvalidSubcommand,
                    "Valid subcommands are `c8y`, `aws` and `az`",
                )
            }),
            None => Err(clap::Error::raw(
                clap::error::ErrorKind::MissingSubcommand,
                "Valid subcommands are `c8y`, `aws` and `az`",
            )),
        }
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl clap::Subcommand for MapperName {
    fn augment_subcommands(cmd: clap::Command) -> clap::Command {
        cmd.subcommand(Command::new("c8y"))
            .subcommand(Command::new("c8y@<profile>"))
            .subcommand_required(true)
            .allow_external_subcommands(true)
    }

    fn augment_subcommands_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_subcommands(cmd)
    }

    fn has_subcommand(name: &str) -> bool {
        name.parse::<Self>().is_ok()
    }
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az(None) => write!(f, "tedge-mapper-az"),
            MapperName::Az(Some(profile)) => write!(f, "tedge-mapper-az@{profile}"),
            MapperName::Aws(None) => write!(f, "tedge-mapper-aws"),
            MapperName::Aws(Some(profile)) => write!(f, "tedge-mapper-aws@{profile}"),
            MapperName::C8y(None) => write!(f, "tedge-mapper-c8y"),
            MapperName::C8y(Some(profile)) => write!(f, "tedge-mapper-c8y@{profile}"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
        }
    }
}

pub async fn run(mapper_opt: MapperOpt) -> anyhow::Result<()> {
    let component = lookup_component(mapper_opt.name.clone());

    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&mapper_opt.common.config_dir);
    let config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

    log_init(
        "tedge-mapper",
        &mapper_opt.common.log_args,
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
            .start(config, mapper_opt.common.config_dir.as_ref())
            .await
    }
}
