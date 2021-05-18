use c8y_mapper::mapper;
use mqtt_client::Client;
use std::str::FromStr;
use structopt::*;
use tedge_dm_agent::monitor::DeviceMonitor;
use tracing::{debug_span, info, warn, Instrument};

const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Cli {
    pub mappers: Vec<MapperName>,
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
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        // .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    let cli = Cli::from_args();

    // Temporary guard to avoid multiple mappers to spawn. To be removed at second stage when handling multiple mappers in one processes.
    if cli.mappers.len() != 1 {
        warn!("You are trying to use experimental feature (run multiple mappers from single process)!!! USE IT AT YOUR OWN RISK.");
    }

    let mut handles = Vec::new();
    for mapper in cli.mappers {
        handles.push(tokio::task::spawn(
            async move { start_mapper(mapper).await },
        ));
    }

    for handle in handles {
        if let Err(err) = handle.await {
            warn!("Error: {}", err)
        }
    }

    Ok(())
}

async fn start_mapper(mapper_name: MapperName) -> Result<(), anyhow::Error> {
    match mapper_name {
        MapperName::Az => {
            unimplemented!("Will add it after Rina merges")
        }

        MapperName::C8y => {
            const APP_NAME: &str = "c8y-mapper";
            info!("{} starting!", APP_NAME);
            let config = mqtt_client::Config::default();
            let mqtt = Client::connect(APP_NAME, &config).await?;

            let mapper = c8y_mapper::mapper::Mapper::new_from_string(
                mqtt,
                mapper::IN_TOPIC,
                mapper::C8Y_TOPIC_C8Y_JSON,
                mapper::ERRORS_TOPIC,
            )?;

            mapper.run().instrument(debug_span!(APP_NAME)).await?;
            Ok(())
        }

        MapperName::Dm => {
            const APP_NAME: &str = "tedge-dm-agent";

            info!("{} starting!", APP_NAME);
            DeviceMonitor::run()
                .instrument(debug_span!(APP_NAME))
                .await?;
            Ok(())
        }
    }
}
