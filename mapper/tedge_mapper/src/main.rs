use crate::error::MapperError;
use crate::mapper::Mapper;
use mqtt_client::Client;
use tracing::{debug_span, info, Instrument};

mod az_mapper;
mod c8y_mapper;
mod error;
mod mapper;

const APP_NAME: &str = "tedge-mapper";
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

    info!("{} starting!", APP_NAME);

    let config = mqtt_client::Config::default();
    let mqtt = Client::connect(APP_NAME, &config).await?;

    let mapper: Mapper = match cloud_name {
        "c8y" => mapper::Mapper::new(
            mqtt,
            c8y_mapper::CumulocityMapperConfig::default(),
            Box::new(c8y_mapper::CumulocityConverter),
        ),
        "az" => mapper::Mapper::new(
            mqtt,
            az_mapper::AzureMapperConfig::default(),
            Box::new(az_mapper::AzureConverter {
                add_timestamp: true,
            }),
        ),
        _ => return Err(MapperError::IncorrectArgument),
    };

    mapper.run().instrument(debug_span!(APP_NAME)).await?;

    Ok(())
}
