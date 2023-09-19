//! Handles cloud-agnostic operations.
//!
//! The Tedge Agent addresses cloud-agnostic software management operations e.g.
//! listing current installed software list, software update, software removal.
//! Also, the Tedge Agent calls an SM Plugin(s) to execute an action defined by
//! a received operation.
//!
//! It also has following capabilities:
//!
//! - File transfer HTTP server
//! - Restart management
//! - Software management

use std::sync::Arc;

use agent::AgentConfig;
use camino::Utf8PathBuf;
use clap::Parser;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tracing::log::warn;

mod agent;
mod file_transfer_server;
mod restart_manager;
mod software_manager;
mod state_repository;
mod tedge_operation_converter;

#[derive(Debug, Clone, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct AgentOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG
    #[clap(long)]
    pub debug: bool,

    /// Start the agent with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    pub init: bool,

    /// Start the agent from custom path
    ///
    /// WARNING: This is mostly used in testing.
    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: Utf8PathBuf,

    /// The device MQTT topic identifier
    #[clap(long)]
    pub mqtt_device_topic_id: Option<Arc<str>>,

    /// MQTT root prefix
    #[clap(long)]
    pub mqtt_topic_root: Option<Arc<str>>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let agent_opt = AgentOpt::parse();
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(agent_opt.config_dir.clone());

    // If `debug` is `false` then only `error!`, `warn!` and `info!` are reported.
    // If `debug` is `true` then also `debug!` is reported.
    let log_level = if agent_opt.debug {
        tracing::Level::DEBUG
    } else {
        get_log_level("tedge-agent", &tedge_config_location.tedge_config_root_path)?
    };

    set_log_level(log_level);

    let init = agent_opt.init;

    let mut agent = agent::Agent::try_new(
        "tedge-agent",
        AgentConfig::from_config_and_cliopts(&tedge_config_location, agent_opt)?,
    )?;

    if init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        return Ok(());
    } else {
        agent.start().await?;
    }
    Ok(())
}
