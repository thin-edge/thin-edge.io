//! Entry point for `tedge run all` — assembles the units and runs the supervisor.

use anyhow::Context;
use futures::FutureExt;
use std::collections::HashSet;
use tedge_agent::AgentOpt;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init_reloadable_for_services;
use tedge_config::TEdgeConfig;
use tedge_mapper::MapperName;
use tedge_supervisor::RuntimeFactory;
use tedge_supervisor::Supervisor;
use tedge_supervisor::Unit;
use tedge_supervisor::UnitKind;

/// `tedge run all` — run the agent and mappers under one supervisor.
///
/// Mappers are specified as trailing positional arguments in `cloud[@profile]`
/// format, e.g. `tedge run all c8y aws c8y@secondary`. When none are given, every
/// mapper configured in `tedge.toml` (plus user-defined mapper directories) is run.
#[derive(Debug, clap::Parser)]
pub struct RunAllOpt {
    /// Mappers to run alongside the agent (e.g. `c8y`, `aws`, `c8y@profile`).
    #[arg(num_args = 1..)]
    pub mappers: Vec<String>,

    #[command(flatten)]
    pub common: CommonArgs,
}

/// Entry point for `tedge run all`: assembles the units and runs the supervisor.
pub async fn run(opt: RunAllOpt) -> anyhow::Result<()> {
    let config_dir = opt.common.config_dir.clone();
    let tedge_config = TEdgeConfig::load(&config_dir).await?;

    let mappers = if opt.mappers.is_empty() {
        tedge_mapper::configured_mappers(&tedge_config).await
    } else {
        parse_mapper_args(&opt.mappers)?
    };

    let log_services = log_service_names(&mappers);
    let log_services: Vec<_> = log_services.iter().map(String::as_str).collect();
    let log_reload = log_init_reloadable_for_services(
        &log_services,
        &opt.common.log_args,
        &opt.common.config_dir,
    )?;

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

    // Mapper units — spawned after the agent in the order given.
    for mapper in mappers {
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

/// Parses the mapper name args given on the command line, rejecting any mapper that is
/// specified more than once (each mapper holds a per-service lock and MQTT identity,
/// so a duplicate cannot run in the same process)
fn parse_mapper_args(args: &[String]) -> anyhow::Result<Vec<MapperName>> {
    let mut seen = HashSet::new();
    let mut mappers = Vec::new();
    for arg in args {
        let mapper = MapperName::parse_cli_arg(arg)
            .with_context(|| format!("parsing mapper spec '{arg}'"))?;
        anyhow::ensure!(
            seen.insert(mapper.to_string()),
            "mapper '{arg}' is specified more than once"
        );
        mappers.push(mapper);
    }
    Ok(mappers)
}

/// One name per hosted unit — the `tedge` fallback and the generic `tedge-mapper`
/// inheritance are resolved inside the filter. Keeping the list to the concrete
/// components makes it single-element exactly when the supervisor hosts one unit
/// and so runs it without a `component` span: the filter then applies that
/// component's level process-wide instead of relying on span attribution.
fn log_service_names(mappers: &[MapperName]) -> Vec<String> {
    let mut services = vec![tedge_agent::AGENT_NAME.to_string()];
    for mapper in mappers {
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
            log_service_names(&[MapperName::Collectd]),
            vec![
                tedge_agent::AGENT_NAME.to_string(),
                "tedge-mapper-collectd".to_string(),
            ]
        );
    }

    #[test]
    fn run_all_logging_includes_all_mappers() {
        assert_eq!(
            log_service_names(&[MapperName::Collectd, MapperName::C8y { profile: None }]),
            vec![
                tedge_agent::AGENT_NAME.to_string(),
                "tedge-mapper-collectd".to_string(),
                "tedge-mapper-c8y".to_string(),
            ]
        );
    }

    #[test]
    fn run_all_without_a_mapper_applies_the_agent_log_level_process_wide() {
        // A single-element list is what makes the component log filter fold the
        // agent's configured level into the process-wide default, matching the
        // supervisor dropping the `component` span when it hosts a single unit.
        assert_eq!(
            log_service_names(&[]),
            vec![tedge_agent::AGENT_NAME.to_string()]
        );
    }

    #[test]
    fn trailing_arguments_parse_as_mapper_specs() {
        use clap::Parser;
        let opt = RunAllOpt::try_parse_from(["tedge-run-all", "c8y", "aws"]).unwrap();
        assert_eq!(opt.mappers, ["c8y", "aws"]);
    }

    #[test]
    fn distinct_mappers_and_profiles_are_accepted() {
        let mappers = parse_mapper_args(&["c8y".to_string(), "c8y@secondary".to_string()]).unwrap();
        let names: Vec<_> = mappers.iter().map(MapperName::to_string).collect();
        assert_eq!(names, ["tedge-mapper-c8y", "tedge-mapper-c8y@secondary"]);
    }

    #[test]
    fn duplicate_mapper_specs_are_rejected() {
        let err = parse_mapper_args(&["c8y".to_string(), "c8y".to_string()]).unwrap_err();
        assert!(
            format!("{err}").contains("more than once"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn flags_after_a_mapper_spec_are_accepted() {
        use clap::Parser;
        let opt = RunAllOpt::try_parse_from(["tedge-run-all", "c8y", "--debug"]).unwrap();
        assert_eq!(opt.mappers, ["c8y"]);
        assert!(opt.common.log_args.debug);
    }
}
