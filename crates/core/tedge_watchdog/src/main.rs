use clap::Parser;
use std::path::PathBuf;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;

mod error;
mod systemd_watchdog;

#[derive(Debug, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct WatchdogOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long)]
    pub debug: bool,

    /// Start the watchdog from custom path
    ///
    /// WARNING: This is mostly used in testing.
    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let watchdog_opt = WatchdogOpt::parse();
    tedge_utils::logging::initialise_tracing_subscriber(watchdog_opt.debug);

    systemd_watchdog::start_watchdog(watchdog_opt.config_dir).await?;
    Ok(())
}
