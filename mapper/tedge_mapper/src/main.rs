use crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper;
use crate::{
    az_mapper::AzureMapper, c8y_mapper::CumulocityMapper, collectd_mapper::mapper::CollectdMapper,
    component::TEdgeComponent, error::*,
};
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
mod size_threshold;
mod sm_c8y_mapper;

const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mapper = MapperOpt::from_args();
    initialise_logging(mapper.debug);

    let component = lookup_component(&mapper.name);

    let config = tedge_config()?;
    component.start(config).await
}

fn initialise_logging(debug: bool) {
    let log_level = if debug {
        tracing::Level::TRACE
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_max_level(log_level)
        .init();
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
