use clap::Parser;
use std::path::PathBuf;
use tedge_config::{system_services::*, DEFAULT_TEDGE_CONFIG_PATH};

mod error;

// on linux, we use systemd
#[cfg(target_os = "linux")]
mod systemd_watchdog;
#[cfg(target_os = "linux")]
use systemd_watchdog as watchdog;

// on non-linux, we do nothing for now
#[cfg(not(target_os = "linux"))]
mod dummy_watchdog;
#[cfg(not(target_os = "linux"))]
use dummy_watchdog as watchdog;

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
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(watchdog_opt.config_dir.clone());

    let log_level = if watchdog_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "tedge_watchdog",
            tedge_config_location.tedge_config_root_path.to_path_buf(),
        )?
    };

    set_log_level(log_level);

    watchdog::start_watchdog(watchdog_opt.config_dir).await
}
