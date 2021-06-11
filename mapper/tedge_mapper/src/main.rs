use crate::error::*;
use crate::size_threshold::*;
use clock::WallClock;
use flockfile::{Flockfile, FlockfileError};
use mqtt_client::{Client, Config};
use std::path::PathBuf;
use std::str::FromStr;
use tedge_config::*;
use tracing::{debug_span, error, info, Instrument};

mod az_converter;
mod az_mapper;
mod c8y_converter;
mod c8y_mapper;
mod converter;
mod error;
mod mapper;
mod size_threshold;

const APP_NAME_C8Y: &str = "tedge-mapper-c8y";
const APP_NAME_AZ: &str = "tedge-mapper-az";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Only one argument is allowed
    if args.len() != 2 {
        return Err(MapperError::IncorrectArgument.into());
    }

    let cloud_name = CloudName::from_str(args[1].as_str())?;

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    match cloud_name {
        CloudName::C8y => {
            let _flockfile = check_another_instance_is_running(APP_NAME_C8Y)?;

            info!("{} starting!", APP_NAME_C8Y);

            let mqtt = Client::connect(APP_NAME_C8Y, &mqtt_config()?).await?;

            mapper::Mapper::new(
                mqtt,
                c8y_mapper::CumulocityMapperConfig::default(),
                Box::new(c8y_converter::CumulocityConverter),
            )
            .run()
            .instrument(debug_span!(APP_NAME_C8Y))
            .await?
        }
        CloudName::Azure => {
            let _flockfile = check_another_instance_is_running(APP_NAME_AZ)?;

            info!("{} starting!", APP_NAME_AZ);

            let mqtt = Client::connect(APP_NAME_AZ, &mqtt_config()?).await?;

            mapper::Mapper::new(
                mqtt,
                az_mapper::AzureMapperConfig::default(),
                Box::new(az_converter::AzureConverter {
                    add_timestamp: tedge_config()?.query(AzureMapperTimestamp)?.is_set(),
                    clock: Box::new(WallClock),
                    size_threshold: SizeThreshold(255 * 1024),
                }),
            )
            .run()
            .instrument(debug_span!(APP_NAME_AZ))
            .await?
        }
    };

    Ok(())
}

fn mqtt_config() -> Result<Config, anyhow::Error> {
    Ok(Config::default().with_port(tedge_config()?.query(MqttPortSetting)?.into()))
}

fn tedge_config() -> Result<TEdgeConfig, anyhow::Error> {
    let config_repository = config_repository()?;
    Ok(config_repository.load()?)
}

pub enum CloudName {
    Azure,
    C8y,
}

impl FromStr for CloudName {
    type Err = MapperError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "c8y" => Ok(CloudName::C8y),
            "az" => Ok(CloudName::Azure),
            _ => Err(MapperError::IncorrectArgument),
        }
    }
}

fn check_another_instance_is_running(app_name: &str) -> Result<Flockfile, FlockfileError> {
    match flockfile::Flockfile::new_lock(format!("{}.lock", app_name)) {
        Ok(file) => Ok(file),
        Err(err) => {
            error!("Another instance of {} is running.", app_name);
            Err(err)
        }
    }
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
