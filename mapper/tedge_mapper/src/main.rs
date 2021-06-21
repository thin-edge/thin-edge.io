use crate::az_mapper::AzureMapper;
use crate::c8y_mapper::CumulocityMapper;
use crate::component::TEdgeComponent;
use crate::error::*;
use std::path::PathBuf;
use std::str::FromStr;
use strum_macros::*;
use tedge_config::*;

mod az_converter;
mod az_mapper;
mod c8y_converter;
mod c8y_mapper;
mod component;
mod converter;
mod error;
mod mapper;
mod size_threshold;

const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[derive(EnumString)]
pub enum ComponentName {
    #[strum(serialize = "az")]
    Azure,

    #[strum(serialize = "c8y")]
    C8y,
}

fn lookup_component(component_name: &ComponentName) -> Box<dyn TEdgeComponent> {
    match component_name {
        ComponentName::C8y => Box::new(CumulocityMapper::new()),
        ComponentName::Azure => Box::new(AzureMapper::new()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Only one argument is allowed
    if args.len() != 2 {
        return Err(MapperError::IncorrectArgument.into());
    }

    initialise_logging();

    let component_name = ComponentName::from_str(&args[1])?;
    let component = lookup_component(&component_name);

    let config = tedge_config()?;
    Ok(component.start(config).await?)
}

fn initialise_logging() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
}

fn tedge_config() -> Result<TEdgeConfig, anyhow::Error> {
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

// Copied from tedge/src/utils/paths.rs. In the future, it would be good to separate it from tedge crate.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from)
}
