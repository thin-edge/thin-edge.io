use mqtt_client::Client;
use tracing::{debug_span, info, Instrument};

mod mapper;

const APP_NAME: &str = "tedge-mapper";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[cfg(feature = "memory-statistics")]
#[global_allocator]
static GLOBAL: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

#[tokio::main]
async fn main() -> Result<(), mqtt_client::Error> {
    setup_tracing();
    info!("{} starting!", APP_NAME);

    let mut config = mqtt_client::Config::default();
    config.host = std::env::var("MAPPER_MQTT_HOST").unwrap_or_else(|_| "localhost".into());
    let mqtt = std::sync::Arc::new(Client::connect(APP_NAME, &config).await?);

    #[cfg(feature = "memory-statistics")]
    start_statistics_task(mqtt.clone(), std::time::Duration::from_secs(1));

    let mapper = mapper::Mapper::new_from_string(
        mqtt,
        mapper::IN_TOPIC,
        mapper::C8Y_TOPIC_C8Y_JSON,
        mapper::ERRORS_TOPIC,
    )?;

    mapper.run().instrument(debug_span!(APP_NAME)).await?;

    Ok(())
}

fn setup_tracing() {
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        // WARNING: `with_env_filter` accounts for ~800 KiB
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into()))
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();
}

#[cfg(feature = "memory-statistics")]
fn start_statistics_task(mqtt: std::sync::Arc<mqtt_client::Client>, interval: std::time::Duration) {
    let topic_allocated_bytes = mqtt_client::Topic::new("SYS/mapper/bytes/allocated").unwrap();
    let _ = tokio::spawn(async move {
        loop {
            let stats = GLOBAL.stats();
            let active = (stats.bytes_allocated - stats.bytes_deallocated) / 1024;
            let msg = format!("{} KiB", active);
            let _ = mqtt
                .publish(mqtt_client::Message::new(&topic_allocated_bytes, msg))
                .await;
            tokio::time::sleep(interval).await;
        }
    });
}
