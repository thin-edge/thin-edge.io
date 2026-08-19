use async_trait::async_trait;
use tedge_actors::Runtime;
use tedge_api::service_command::ServiceDeployment;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::TedgePaths;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    /// Rebuildable assembly shared by the standalone runner and the supervisor: wires
    /// every actor and spawns the runtime, but installs no signal handling and does
    /// not run to completion. The supervisor owns signals centrally and applies a
    /// restart policy. Safe to call repeatedly for a fresh incarnation.
    ///
    /// `deployment` tells whether this mapper has an init unit of its own or is hosted by
    /// `tedge run all`, which is what decides the actions it declares on its own service.
    async fn build(
        &self,
        tedge_config: TEdgeConfig,
        cfg_dir: &TedgePaths,
        deployment: ServiceDeployment,
    ) -> Result<Runtime, anyhow::Error>;
}
