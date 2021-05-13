mod batcher;
mod collectd;
mod monitor;

use tracing::{debug_span, info, Instrument};

use crate::monitor::DeviceMonitor;

const APP_NAME: &str = "tedge-dm-agent";
const DEFAULT_LOG_LEVEL: &str = "warn";
const TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%:z";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| DEFAULT_LOG_LEVEL.into());
    tracing_subscriber::fmt()
        .with_timer(tracing_subscriber::fmt::time::ChronoUtc::with_format(
            TIME_FORMAT.into(),
        ))
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    info!("{} starting!", APP_NAME);
    DeviceMonitor::run()
        .instrument(debug_span!(APP_NAME))
        .await?;

    Ok(())
}
