use mqtt_client::Client;
use tracing::{debug_span, error, info, Instrument};

mod mapper;

const APP_NAME: &str = "tedge-mapper";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    info!("{} starting!", APP_NAME);

    let _lockfile = match flockfile::Flockfile::new_lock(format!("{}.lock", APP_NAME)) {
        Ok(file) => file,
        Err(err) => {
            error!("Another instance of {} is running.", APP_NAME);
            return Err(err.into());
        }
    };

    let config = mqtt_client::Config::default();
    let mqtt = Client::connect(APP_NAME, &config).await?;

    let mapper = mapper::Mapper::new_from_string(
        mqtt,
        mapper::IN_TOPIC,
        mapper::C8Y_TOPIC_C8Y_JSON,
        mapper::ERRORS_TOPIC,
    )?;

    mapper.run().instrument(debug_span!(APP_NAME)).await?;

    Ok(())
}
