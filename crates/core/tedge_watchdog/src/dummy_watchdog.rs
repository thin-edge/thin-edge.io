use std::path::Path;

pub async fn start_watchdog(_: tedge_config::TEdgeConfig) -> Result<(), anyhow::Error> {
    Err(anyhow::Error::from(WatchdogError::WatchdogNotAvailable))
}

#[derive(Debug, thiserror::Error)]
pub enum WatchdogError {
    #[error("The watchdog is not available on this platform")]
    WatchdogNotAvailable,
}
