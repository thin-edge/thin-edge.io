use async_trait::async_trait;
use tedge_actors::Runtime;
use tedge_config::TEdgeConfig;
use tedge_supervisor::SupervisorMode;
use tedge_utils::paths::TedgePaths;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    /// Rebuildable assembly shared by the standalone runner and the supervisor: wires
    /// every actor and spawns the runtime, but installs no signal handling and does
    /// not run to completion. The supervisor owns signals centrally and applies a
    /// restart policy. Safe to call repeatedly for a fresh incarnation.
    async fn build(
        &self,
        tedge_config: TEdgeConfig,
        cfg_dir: &TedgePaths,
        supervisor_mode: SupervisorMode,
    ) -> Result<Runtime, anyhow::Error>;
}
