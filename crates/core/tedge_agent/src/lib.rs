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
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use tedge_actors::Runtime;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::TEdgeConfig;
use tracing::info;

mod agent;
mod device_profile_manager;
mod entity_manager;
mod http_server;
mod operation_workflows;
mod restart_manager;
mod software_manager;
mod state_repository;
mod twin_manager;

#[derive(Debug, Clone, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!()
)]
pub struct AgentOpt {
    #[command(flatten)]
    pub common: CommonArgs,

    /// The device MQTT topic identifier
    #[clap(long)]
    pub mqtt_device_topic_id: Option<Arc<str>>,

    /// MQTT root prefix
    #[clap(long)]
    pub mqtt_topic_root: Option<Arc<str>>,
}

pub async fn run(
    agent_opt: AgentOpt,
    tedge_config: tedge_config::TEdgeConfig,
) -> Result<(), anyhow::Error> {
    log_init(
        "tedge-agent",
        &agent_opt.common.log_args,
        &agent_opt.common.config_dir,
    )?;

    // Single-instance lock held for the lifetime of the standalone agent.
    let _flock = acquire_lock(&tedge_config)?;
    info!("{AGENT_NAME} starting");

    let config = AgentConfig::from_config_and_cliopts(tedge_config, agent_opt).await?;
    let agent = agent::Agent::new(config);
    agent.start().await?;

    Ok(())
}

/// Name under which the agent registers its single-instance lock and service.
pub const AGENT_NAME: &str = "tedge-agent";

/// Rebuildable factory the single-process supervisor calls (on each restart) for the
/// agent unit. Builds the runtime with no signal handling, lock, or run-to-completion
/// — the supervisor owns those.
pub async fn build(
    agent_opt: AgentOpt,
    tedge_config: TEdgeConfig,
) -> Result<Runtime, anyhow::Error> {
    let config = AgentConfig::from_config_and_cliopts(tedge_config, agent_opt).await?;
    agent::Agent::new(config).build().await
}

/// Acquires the agent's single-instance lock, if locking is enabled.
///
/// Held for the agent's whole lifetime. The supervisor takes it once and retains it
/// across restarts, so it guards only against an external duplicate (e.g. a
/// systemd-managed agent left running).
pub fn acquire_lock(tedge_config: &TEdgeConfig) -> Result<Option<Flockfile>, FlockfileError> {
    if tedge_config.run.lock_files {
        check_another_instance_is_not_running(AGENT_NAME, tedge_config.run.path.as_std_path())
    } else {
        Ok(None)
    }
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
