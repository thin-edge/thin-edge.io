#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use clap::Parser;
use std::future::Future;
use std::path::PathBuf;
use tedge::command::BuildCommand;
use tedge::command::BuildContext;
use tedge::Component;
use tedge::TEdgeOptMulticall;
use tedge_apt_plugin::AptCli;
use tedge_config::system_services::set_log_level;

fn main() -> anyhow::Result<()> {
    let executable_name = executable_name();

    if matches!(executable_name.as_deref(), Some("apt" | "tedge-apt-plugin")) {
        let try_opt = AptCli::try_parse();
        tedge_apt_plugin::run_and_exit(try_opt);
    }

    let opt = parse_multicall_if_known(&executable_name);
    match opt {
        TEdgeOptMulticall::Component(Component::TedgeMapper(mapper_opt)) => {
            block_on(tedge_mapper::run(mapper_opt))
        }
        TEdgeOptMulticall::Component(Component::TedgeAgent(opt)) => {
            block_on(tedge_agent::run(opt, false))
        }
        TEdgeOptMulticall::Component(Component::TedgeAgentV1(opt)) => {
            block_on(tedge_agent::run(opt, true))
        }
        TEdgeOptMulticall::Component(Component::TedgeLogPlugin(opt)) => {
            block_on(tedge_log_plugin::run(opt))
        }
        TEdgeOptMulticall::Component(Component::TedgeConfigurationPlugin(cp_opt)) => {
            block_on(tedge_configuration_plugin::run(cp_opt))
        }
        TEdgeOptMulticall::Component(Component::C8yFirmwarePlugin(fp_opt)) => {
            block_on(c8y_firmware_plugin::run(fp_opt))
        }
        TEdgeOptMulticall::Component(Component::C8yRemoteAccessPlugin(opt)) => {
            block_on(c8y_remote_access_plugin::run(opt)).unwrap();
            Ok(())
        }
        TEdgeOptMulticall::Component(Component::TedgeWatchdog(opt)) => {
            block_on(tedge_watchdog::run(opt))
        }
        TEdgeOptMulticall::Component(Component::TedgeWrite(opt)) => tedge_write::bin::run(opt),
        TEdgeOptMulticall::Tedge { cmd, config_dir } => {
            set_log_level(tracing::Level::WARN);

            let tedge_config_location =
                tedge_config::TEdgeConfigLocation::from_custom_root(config_dir);
            let config_repository =
                tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

            let build_context = BuildContext {
                config_repository,
                config_location: tedge_config_location,
            };
            let cmd = cmd
                .build_command(build_context)
                .with_context(|| "missing configuration parameter")?;

            cmd.execute()
                .with_context(|| format!("failed to {}", cmd.description()))
        }
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

fn executable_name() -> Option<String> {
    Some(
        PathBuf::from(std::env::args_os().next()?)
            .file_stem()?
            .to_str()?
            .to_owned(),
    )
}

fn parse_multicall_if_known<T: Parser>(executable_name: &Option<String>) -> T {
    let cmd = T::command();

    let is_known_subcommand = executable_name
        .as_deref()
        .map_or(false, |name| cmd.find_subcommand(name).is_some());
    let cmd = cmd.multicall(is_known_subcommand);
    T::from_arg_matches(&cmd.get_matches()).expect("get_matches panics if invalid arguments are provided, so we won't have arg matches to convert")
}
