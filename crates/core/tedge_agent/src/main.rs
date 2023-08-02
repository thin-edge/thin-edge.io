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
mod tedge_to_te_converter;

#[derive(Debug, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct AgentOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
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
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let agent_opt = AgentOpt::parse();
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(agent_opt.config_dir.clone());

    // If `debug` is `false` then only `error!`, `warn!` and `info!` are reported.
    // If `debug` is `true` then only `debug!` and `trace!` are reported.
    let log_level = if agent_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level("tedge-agent", &tedge_config_location.tedge_config_root_path)?
    };

    set_log_level(log_level);

    let mut agent = agent::Agent::try_new(
        "tedge-agent",
        AgentConfig::from_tedge_config(&tedge_config_location)?,
    )?;
    if agent_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        return Ok(());
    } else {
        agent.start().await?;
    }
    Ok(())
}
