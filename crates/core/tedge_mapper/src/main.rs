use std::fmt;

use crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper;
use crate::{
    az_mapper::AzureMapper, c8y_mapper::CumulocityMapper, collectd_mapper::mapper::CollectdMapper,
    component::TEdgeComponent, error::*,
};
use flockfile::check_another_instance_is_not_running;
use structopt::*;
use tedge_config::*;
use tedge_utils::paths::home_dir;

mod az_converter;
mod az_mapper;
mod c8y_converter;
mod c8y_mapper;
mod collectd_mapper;
mod component;
mod converter;
mod error;
mod mapper;
mod operations;
mod size_threshold;
mod sm_c8y_mapper;

fn lookup_component(component_name: &MapperName) -> Box<dyn TEdgeComponent> {
    match component_name {
        MapperName::Az => Box::new(AzureMapper::new()),
        MapperName::Collectd => Box::new(CollectdMapper::new()),
        MapperName::C8y => Box::new(CumulocityMapper::new()),
        MapperName::SmC8y => Box::new(CumulocitySoftwareManagementMapper::new()),
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
    #[structopt(long)]
    pub debug: bool,
}

#[derive(Debug, StructOpt)]
pub enum MapperName {
    Az,
    C8y,
    Collectd,
    SmC8y,
}

impl fmt::Display for MapperName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MapperName::Az => write!(f, "{}", "tedge-mapper-az"),
            MapperName::C8y => write!(f, "{}", "tedge-mapper-c8y"),
            MapperName::Collectd => write!(f, "{}", "tedge-mapper-collectd"),
            MapperName::SmC8y => write!(f, "{}", "sm-c8y-mapper"),
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

    component.start(config).await
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
