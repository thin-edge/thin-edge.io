use crate::aws::mapper::AwsMapper;
use crate::az::mapper::AzureMapper;
use crate::c8y::mapper::CumulocityMapper;
use crate::collectd::mapper::CollectdMapper;
use crate::core::component::TEdgeComponent;
use clap::Parser;
use flockfile::check_another_instance_is_not_running;
use std::fmt;
use std::path::PathBuf;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tracing::log::warn;

mod aws;
mod az;
mod c8y;
mod collectd;
mod core;

fn lookup_component(component_name: &MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az => Box::new(AzureMapper::new()),
        MapperName::Aws => Box::new(AwsMapper),
        MapperName::Collectd => Box::new(CollectdMapper),
        MapperName::C8y => Box::new(CumulocityMapper),
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

    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long, global = true)]
    pub debug: bool,

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
    /// WARNING: This is mostly used in testing.
    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper_opt = MapperOpt::parse();

    let component = lookup_component(&mapper_opt.name);

    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&mapper_opt.config_dir);
    let config =
        tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone()).load_new()?;

    let log_level = if mapper_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "tedge-mapper",
            &tedge_config_location.tedge_config_root_path,
        )?
    };
    set_log_level(log_level);

    // Run only one instance of a mapper (if enabled)
    let mut _flock = None;
    if config.run.lock_files {
        let run_dir = config.run.path.as_std_path();
        _flock = Some(check_another_instance_is_not_running(
            &mapper_opt.name.to_string(),
            run_dir,
        )?);
    }

    if mapper_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else if mapper_opt.clear {
        warn!("This --clear option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        component.start(config, &mapper_opt.config_dir).await
    }
}
