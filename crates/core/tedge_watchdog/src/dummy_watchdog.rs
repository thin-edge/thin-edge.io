use std::path::PathBuf;

pub async fn start_watchdog(_config_dir: PathBuf) -> Result<(), anyhow::Error> {
    Err(anyhow::Error::from(
        crate::error::WatchdogError::WatchdogNotAvailable,
    ))
}
