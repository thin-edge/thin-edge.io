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
use tedge_config::cli::CommonArgs;
use tedge_config::system_services::log_init;
use tracing::log::warn;

mod agent;
mod device_profile_manager;
mod file_transfer_server;
mod operation_file_cache;
mod operation_workflows;
mod restart_manager;
mod software_manager;
mod state_repository;
mod tedge_to_te_converter;

#[derive(Debug, Clone, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct AgentOpt {
    /// Start the agent with clean session off, subscribe to the topics, so that no messages are lost
    #[clap(short, long)]
    pub init: bool,

    #[command(flatten)]
    pub common: CommonArgs,

    /// The device MQTT topic identifier
    #[clap(long)]
    pub mqtt_device_topic_id: Option<Arc<str>>,

    /// MQTT root prefix
    #[clap(long)]
    pub mqtt_topic_root: Option<Arc<str>>,
}

pub async fn run(agent_opt: AgentOpt) -> Result<(), anyhow::Error> {
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(agent_opt.common.config_dir.clone());

    log_init(
        "tedge-agent",
        &agent_opt.common.log_args,
        &tedge_config_location.tedge_config_root_path,
    )?;

    let init = agent_opt.init;

    let agent = agent::Agent::try_new(
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

#[derive(Debug, serde::Deserialize, Clone)]
pub struct Capabilities {
    config_update: bool,
    config_snapshot: bool,
    log_upload: bool,
}

#[cfg(test)]
impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            config_update: true,
            config_snapshot: true,
            log_upload: true,
        }
    }
}
