use std::fmt;

use crate::{
    az::mapper::AzureMapper,
    c8y::mapper::CumulocityMapper,
    collectd::mapper::CollectdMapper,
    core::{component::TEdgeComponent, error::MapperError},
};

use flockfile::check_another_instance_is_not_running;
use structopt::*;
use tedge_config::*;
use tedge_utils::paths::home_dir;

mod az;
mod c8y;
mod collectd;
mod core;

fn lookup_component(component_name: &MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az => Box::new(AzureMapper::new()),
        MapperName::Collectd => Box::new(CollectdMapper::new()),
        MapperName::C8y => Box::new(CumulocityMapper::new()),
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct MapperOpt {
    #[structopt(subcommand)]
    pub name: MapperName,

    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[structopt(long, global = true)]
    pub debug: bool,

    /// Start the mapper with clean session off, subscribe to the topics, so that no messages are lost
    #[structopt(short, long)]
    pub init: bool,

    /// Start the agent with clean session on, drop the previous session and subscriptions
    ///
    /// WARNING: All pending messages will be lost.
    #[structopt(short, long)]
    pub clear: bool,
}

#[derive(Debug, StructOpt)]
pub enum MapperName {
    Az,
    C8y,
    Collectd,
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az => write!(f, "tedge-mapper-az"),
            MapperName::C8y => write!(f, "tedge-mapper-c8y"),
            MapperName::Collectd => write!(f, "tedge-mapper-collectd"),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper = MapperOpt::from_args();
    tedge_utils::logging::initialise_tracing_subscriber(mapper.debug);

    let component = lookup_component(&mapper.name);
    let config = tedge_config()?;
    // Run only one instance of a mapper
    let _flock = check_another_instance_is_not_running(&mapper.name.to_string())?;

    if mapper.init {
        let mut mapper = CumulocityMapper::new();
        mapper.init_session().await
    } else if mapper.clear {
        let mut mapper = CumulocityMapper::new();
        mapper.clear_session().await
    } else {
        component.start(config).await
    }
}

fn tedge_config() -> anyhow::Result<TEdgeConfig> {
    let config_repository = config_repository()?;
    Ok(config_repository.load()?)
}

fn config_repository() -> Result<TEdgeConfigRepository, MapperError> {
    let tedge_config_location = if tedge_users::UserManager::running_as_root()
        || tedge_users::UserManager::running_as("tedge-mapper")
    {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            home_dir().ok_or(MapperError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    Ok(config_repository)
}
