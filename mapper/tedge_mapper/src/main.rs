use crate::error::MapperError;
use crate::time_provider::SystemTimeProvider;
use mqtt_client::Client;
use std::path::PathBuf;
use tedge_config::{
    AzureMapperTimestamp, ConfigRepository, ConfigSettingAccessor, TEdgeConfigRepository,
};
use tracing::{debug_span, info, Instrument};

mod az_mapper;
mod c8y_mapper;
mod error;
mod mapper;
mod time_provider;

const APP_NAME_C8Y: &str = "tedge-mapper-c8y";
const APP_NAME_AZ: &str = "tedge-mapper-az";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[tokio::main]
async fn main() -> Result<(), MapperError> {
    let args: Vec<String> = std::env::args().collect();

    // Only one argument is allowed
    if args.len() != 2 {
        return Err(MapperError::IncorrectArgument);
    }

    let cloud_name = args[1].as_str();

    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    match cloud_name {
        "c8y" => {
            info!("{} starting!", APP_NAME_C8Y);
            let mqtt_config = mqtt_client::Config::default();
            let mqtt = Client::connect(APP_NAME_C8Y, &mqtt_config).await?;

            mapper::Mapper::new(
                mqtt,
                c8y_mapper::CumulocityMapperConfig::default(),
                Box::new(c8y_mapper::CumulocityConverter),
            )
            .run()
            .instrument(debug_span!(APP_NAME_C8Y))
            .await?
        }
        "az" => {
            info!("{} starting!", APP_NAME_AZ);
            let mqtt_config = mqtt_client::Config::default();
            let mqtt = Client::connect(APP_NAME_AZ, &mqtt_config).await?;

            let config_repository = get_config_repository()?;
            let tedge_config = config_repository.load()?;
            mapper::Mapper::new(
                mqtt,
                az_mapper::AzureMapperConfig::default(),
                Box::new(az_mapper::AzureConverter {
                    add_timestamp: tedge_config.query(AzureMapperTimestamp)?.is_set(),
                    time_provider: Box::new(SystemTimeProvider),
                }),
            )
            .run()
            .instrument(debug_span!(APP_NAME_AZ))
            .await?
        }
        _ => return Err(MapperError::IncorrectArgument),
    };

    Ok(())
}

fn get_config_repository() -> Result<TEdgeConfigRepository, MapperError> {
    let tedge_config_location = if running_as_root() {
        tedge_config::TEdgeConfigLocation::from_default_system_location()
    } else {
        tedge_config::TEdgeConfigLocation::from_users_home_location(
            home_dir().ok_or(MapperError::HomeDirNotFound)?,
        )
    };
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location);
    Ok(config_repository)
}

// Copied from tedge/src/utils/users/unix.rs. In the future, it would be good to separate it from tedge crate.
fn running_as_root() -> bool {
    users::get_current_uid() == 0
}

// Copied from tedge/src/utils/paths.rs. In the future, it would be good to separate it from tedge crate.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from)
}
