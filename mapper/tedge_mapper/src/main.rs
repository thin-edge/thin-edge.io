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

const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

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
enum MapperName {
    Az,
    C8y,
    Collectd,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    initialise_logging();

    let component = lookup_component(&MapperName::from_args());

    let config = tedge_config()?;
    component.start(config).await
}

fn initialise_logging() {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
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
