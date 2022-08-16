pub async fn start_watchdog(_config_dir: PathBuf) -> Result<(), anyhow::Error> {
    anyhow::Error::from(crate::error::WatchdogError::WatchdogNotAvailable)
}
