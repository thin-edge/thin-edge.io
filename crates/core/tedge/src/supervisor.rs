//! Entry point for `tedge run all` — assembles the units and runs the supervisor.

use anyhow::Context;
use futures::FutureExt;
use tedge_agent::AgentOpt;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init_reloadable_for_services;
use tedge_config::TEdgeConfig;
use tedge_mapper::MapperName;
use tedge_supervisor::RuntimeFactory;
use tedge_supervisor::Supervisor;
use tedge_supervisor::Unit;
use tedge_supervisor::UnitKind;

/// `tedge run all` — run the agent and (optionally) a mapper under one supervisor.
#[derive(Debug, clap::Parser)]
pub struct RunAllOpt {
    /// The mapper to run alongside the agent (e.g. `c8y`, `aws`, `az`).
    #[clap(subcommand)]
    pub mapper: Option<MapperName>,

    #[command(flatten)]
    pub common: CommonArgs,
}

/// Entry point for `tedge run all`: assembles the units and runs the supervisor.
pub async fn run(opt: RunAllOpt) -> anyhow::Result<()> {
    let log_services = log_service_names(opt.mapper.as_ref());
    let log_services: Vec<_> = log_services.iter().map(String::as_str).collect();
    let log_reload = log_init_reloadable_for_services(
        &log_services,
        &opt.common.log_args,
        &opt.common.config_dir,
    )?;

    let config_dir = opt.common.config_dir.clone();
    let tedge_config = TEdgeConfig::load(&config_dir).await?;

    let mut units: Vec<Unit> = Vec::new();

    // Agent unit — spawned first (best-effort ordering).
    {
        let lock = tedge_agent::acquire_lock(&tedge_config).context("acquiring agent lock")?;
        let agent_opt = AgentOpt {
            common: opt.common.clone(),
            mqtt_device_topic_id: None,
            mqtt_topic_root: None,
        };
        let config_dir = config_dir.clone();
        let factory: RuntimeFactory = Box::new(move || {
            let config_dir = config_dir.clone();
            let agent_opt = agent_opt.clone();
            async move {
                let config = TEdgeConfig::load(&config_dir).await?;
                tedge_agent::build(agent_opt, config).await
            }
            .boxed()
        });
        units.push(Unit::new(
            tedge_agent::AGENT_NAME.to_string(),
            UnitKind::Agent,
            factory,
            lock,
        ));
    }

    // Mapper unit — optional, spawned after the agent.
    if let Some(mapper) = opt.mapper {
        let name = mapper.to_string();
        let lock = tedge_mapper::acquire_lock(&name, &tedge_config)
            .with_context(|| format!("acquiring lock for {name}"))?;
        let config_dir = config_dir.clone();
        let factory: RuntimeFactory = Box::new(move || {
            let config_dir = config_dir.clone();
            let mapper = mapper.clone();
            async move {
                let config = TEdgeConfig::load(&config_dir).await?;
                tedge_mapper::build(mapper, config).await
            }
            .boxed()
        });
        units.push(Unit::new(name, UnitKind::Mapper, factory, lock));
    }

    Supervisor::new(units)
        .with_log_reload(log_reload)
        .run()
        .await
}

/// One name per hosted unit — the `tedge` fallback and the generic `tedge-mapper`
/// inheritance are resolved inside the filter. Keeping the list to the concrete
/// components makes it single-element exactly when the supervisor hosts one unit
/// and so runs it without a `component` span: the filter then applies that
/// component's level process-wide instead of relying on span attribution.
fn log_service_names(mapper: Option<&MapperName>) -> Vec<String> {
    let mut services = vec![tedge_agent::AGENT_NAME.to_string()];
    if let Some(mapper) = mapper {
        services.push(mapper.log_service_name().to_string());
    }
    services
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_all_logging_considers_the_agent_and_mapper_components() {
        assert_eq!(
            log_service_names(Some(&MapperName::Collectd)),
            vec![
                tedge_agent::AGENT_NAME.to_string(),
                "tedge-mapper-collectd".to_string(),
            ]
        );
    }

    #[test]
    fn run_all_without_a_mapper_applies_the_agent_log_level_process_wide() {
        // A single-element list is what makes the component log filter fold the
        // agent's configured level into the process-wide default, matching the
        // supervisor dropping the `component` span when it hosts a single unit.
        assert_eq!(
            log_service_names(None),
            vec![tedge_agent::AGENT_NAME.to_string()]
        );
    }
}
