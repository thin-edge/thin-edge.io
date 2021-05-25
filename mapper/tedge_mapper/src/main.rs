use az_mapper::{az_converter, az_mapper::AzureMapperConfig, size_threshold::SizeThreshold};
use c8y_mapper::{c8y_converter, c8y_mapper::CumulocityMapperConfig};
use clock::WallClock;
use dm_mapper::monitor::{DeviceMonitor, DeviceMonitorConfig};
use flockfile::{Flockfile, FlockfileError};
use mapper::Mapper;
use mapper_converter::{error::MapperError, mapper};
use mqtt_client::Client;
use std::path::PathBuf;
use std::str::FromStr;
use structopt::*;
use tedge_config::{
    AzureMapperTimestamp, ConfigRepository, ConfigSettingAccessor, TEdgeConfigRepository,
};
use tracing::{debug_span, error, info, Instrument};

const APP_NAME_AZ: &str = "tedge-mapper-az";
const APP_NAME_C8Y: &str = "tedge-mapper-c8y";
const APP_NAME_DM: &str = "tedge-dm-agent";
const DEFAULT_LOG_LEVEL: &str = "info";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Cli {
    pub mapper: MapperName,
}

#[derive(Clone, Copy, Debug)]
pub enum MapperName {
    Az,
    C8y,
    Dm,
}

impl FromStr for MapperName {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "az" => Ok(MapperName::Az),
            "c8y" => Ok(MapperName::C8y),
            "dm" => Ok(MapperName::Dm),
            _ => Err("Unknown mapper name."),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    let cli = Cli::from_args();

    match cli.mapper {
        MapperName::C8y => {
            let _flockfile = check_another_instance_is_running(APP_NAME_C8Y)?;

            info!("{} starting!", APP_NAME_C8Y);

            let mqtt_config = mqtt_client::Config::default();
            let mqtt = Client::connect(APP_NAME_C8Y, &mqtt_config).await?;

            Mapper::new(
                mqtt,
                CumulocityMapperConfig::default(),
                Box::new(c8y_converter::CumulocityConverter),
            )
            .run()
            .instrument(debug_span!(APP_NAME_C8Y))
            .await?
        }

        MapperName::Az => {
            let _flockfile = check_another_instance_is_running(APP_NAME_AZ)?;

            info!("{} starting!", APP_NAME_AZ);

            let mqtt_config = mqtt_client::Config::default();
            let mqtt = Client::connect(APP_NAME_AZ, &mqtt_config).await?;

            let config_repository = get_config_repository()?;
            let tedge_config = config_repository.load()?;

            Mapper::new(
                mqtt,
                AzureMapperConfig::default(),
                Box::new(az_converter::AzureConverter {
                    add_timestamp: tedge_config.query(AzureMapperTimestamp)?.is_set(),
                    clock: Box::new(WallClock),
                    size_threshold: SizeThreshold(255 * 1024),
                }),
            )
            .run()
            .instrument(debug_span!(APP_NAME_AZ))
            .await?
        }

        MapperName::Dm => {
            info!("{} starting!", APP_NAME_DM);

            let device_monitor_config = DeviceMonitorConfig::default();
            let device_monitor = DeviceMonitor::new(device_monitor_config);
            device_monitor
                .run()
                .instrument(debug_span!(APP_NAME_DM))
                .await?;
        }
    };

    Ok(())
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
