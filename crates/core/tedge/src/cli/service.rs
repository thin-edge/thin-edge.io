use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;
use std::time::Duration;
use tedge_api::service_command::ActionName;
use tedge_api::service_command::ServiceName;
use tedge_api::service_command::ServiceType;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::workflow::log::logged_command::CmdStatus;
use tedge_api::workflow::BEGIN_TEDGE_MARKER;
use tedge_api::workflow::END_TEDGE_MARKER;
use tedge_api::LoggedCommand;
use tedge_config::get_config_dir;
use tedge_config::models::SecondsOrHumanTime;
use tedge_config::SystemConfig;
use tedge_config::TEdgeConfig;
use tedge_config::SYSTEM_CONFIG_FILE;
use tedge_system_services::service_manager;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceError;
use tedge_utils::signals::TermSignals;

/// The exit code of an action that is not supported for the service type
const NOT_SUPPORTED_EXIT_CODE: i32 = 2;

/// The exit code of an action terminated on timeout, as used by the `timeout` utility
const TIMEOUT_EXIT_CODE: i32 = 124;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeServiceOpt {
    /// The action to run on the service, e.g. start, stop or restart
    ///
    /// Which actions are supported is decided depending on the service type.
    /// Init system for the default service type, a service plugin for any other type.
    #[arg(add(ArgValueCandidates::new(action_completions)))]
    action: ActionName,

    /// The name of the service to act on
    service_name: ServiceName,

    /// The type of the service
    ///
    /// The default type is handled by the init system configured in system.toml.
    /// Any other type is handled by the service plugin.
    #[clap(long("type"), default_value = DEFAULT_SERVICE_TYPE)]
    #[arg(add(ArgValueCandidates::new(service_type_completions)))]
    service_type: ServiceType,

    /// Timeout for the action to complete, after which a graceful termination starts
    // The default covers a restart by systemd,
    // which waits up to 90 seconds to stop a service and up to 90 seconds to start it
    #[clap(long, default_value = "180s")]
    timeout: SecondsOrHumanTime,

    /// Timeout for forced termination, starting after a graceful timeout expires
    #[clap(long, default_value = "60s")]
    forceful_timeout: SecondsOrHumanTime,
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeServiceOpt {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        Ok(ServiceActionCommand {
            action: self.action,
            service_name: self.service_name,
            service_type: self.service_type,
            plugin_paths: config
                .service
                .plugin_paths
                .0
                .iter()
                .map(Utf8PathBuf::from)
                .collect(),
            graceful_timeout: self.timeout.duration(),
            forceful_timeout: self.forceful_timeout.duration(),
        }
        .into_boxed())
    }
}

#[derive(Debug)]
pub struct ServiceActionCommand {
    action: ActionName,
    service_name: ServiceName,
    service_type: ServiceType,
    plugin_paths: Vec<Utf8PathBuf>,
    graceful_timeout: Duration,
    forceful_timeout: Duration,
}

#[async_trait::async_trait]
impl Command for ServiceActionCommand {
    fn description(&self) -> String {
        format!(
            "run the '{}' action on the '{}' service of type '{}'",
            self.action, self.service_name, self.service_type
        )
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.run(&config).await {
            Ok(()) => Ok(()),
            Err(ServiceActionError::NotSupported(reason)) => {
                eprintln!("Error: {reason}");
                std::process::exit(NOT_SUPPORTED_EXIT_CODE)
            }
            Err(err) if err.is_timeout() => {
                eprintln!("Error: {err}");
                std::process::exit(TIMEOUT_EXIT_CODE)
            }
            Err(err) => Err(MaybeFancy::Unfancy(err.into())),
        }
    }
}

impl ServiceActionCommand {
    async fn run(&self, config: &TEdgeConfig) -> Result<(), ServiceActionError> {
        if self.service_type.is_default() {
            self.run_init_system_action(config.root_dir()).await
        } else {
            self.run_plugin_action().await
        }
    }

    async fn run_init_system_action(
        &self,
        config_root: &Utf8Path,
    ) -> Result<(), ServiceActionError> {
        let service_manager = service_manager(config_root)?;
        let service = SystemService::new(self.service_name.as_str());

        let exec_command = service_manager
            .action_command(self.action.as_str(), service)
            .map_err(|err| match err {
                err @ (SystemServiceError::UnsupportedAction { .. }
                | SystemServiceError::NotAnAction { .. }) => {
                    ServiceActionError::NotSupported(err.to_string())
                }
                err => err.into(),
            })?;
        let service_command = exec_command.to_string();

        let command = LoggedCommand::from_command(exec_command.to_command(), ".");
        let output = match self.execute_with_timeout(command).await {
            Ok(output) => output,
            Err(ExecutionError::NotStarted(_)) => {
                return Err(SystemServiceError::ServiceCommandNotFound {
                    service_command,
                    path: config_root.join(SYSTEM_CONFIG_FILE).to_string(),
                }
                .into())
            }
            Err(ExecutionError::Terminated(termination)) => {
                return Err(ServiceActionError::InitSystemTerminated {
                    service_command,
                    termination,
                })
            }
        };

        if output.status.success() {
            return Ok(());
        }

        match output.status.code() {
            Some(code) => Err(ServiceActionError::InitSystemFailed {
                service_command,
                code,
            }),
            None => Err(ServiceActionError::InitSystemKilled { service_command }),
        }
    }

    async fn run_plugin_action(&self) -> Result<(), ServiceActionError> {
        let plugin = self.find_plugin().await?;

        let mut command = LoggedCommand::from_command(std::process::Command::new(&plugin), ".");
        command
            .arg(self.action.as_str())
            .arg(self.service_name.as_str());
        let output = match self.execute_with_timeout(command).await {
            Ok(output) => output,
            Err(ExecutionError::NotStarted(err)) => {
                return Err(ServiceActionError::PluginNotExecutable {
                    plugin,
                    source: err,
                })
            }
            Err(ExecutionError::Terminated(termination)) => {
                return Err(ServiceActionError::PluginTerminated {
                    plugin,
                    termination,
                })
            }
        };

        match output.status.code() {
            Some(0) => Ok(()),
            Some(code) if code == NOT_SUPPORTED_EXIT_CODE => {
                Err(ServiceActionError::NotSupported(format!(
                    "The '{}' action is not supported for the service type '{}'",
                    self.action, self.service_type
                )))
            }
            Some(code) => Err(ServiceActionError::PluginFailed { plugin, code }),
            None => Err(ServiceActionError::PluginKilled { plugin }),
        }
    }

    /// Runs the command of an action, terminating it on timeout or when this process receives SIGINT, SIGTERM or SIGQUIT
    async fn execute_with_timeout(
        &self,
        mut command: LoggedCommand,
    ) -> Result<Output, ExecutionError> {
        // The command runs in a process group of its own, which a Ctrl-C on the terminal does not reach.
        // The signals are caught before spawning the command, so that none can leave it running.
        let mut signals = TermSignals::new(None);
        let child = command.spawn().map_err(ExecutionError::NotStarted)?;
        let interrupted = async move {
            let _ = signals.might_interrupt(std::future::pending::<()>()).await;
        };

        let (output, status) = child
            .wait_for_output_or_terminate(self.graceful_timeout, self.forceful_timeout, interrupted)
            .await
            .map_err(|_| ExecutionError::Terminated(Termination::NotStopped))?;

        forward_output(&output);

        let termination = match status {
            CmdStatus::Successful => return Ok(output),
            CmdStatus::KilledWithSigterm => Termination::Timeout {
                timeout: self.graceful_timeout,
            },
            CmdStatus::KilledWithSigKill => Termination::KilledOnTimeout {
                timeout: self.graceful_timeout,
                forceful_timeout: self.forceful_timeout,
            },
            CmdStatus::KilledWithInterrupt => Termination::Interrupted,
        };
        Err(ExecutionError::Terminated(termination))
    }

    /// Returns the first plugin file found in the configured plugin paths for the service type.
    async fn find_plugin(&self) -> Result<Utf8PathBuf, ServiceActionError> {
        for dir in &self.plugin_paths {
            let plugin = dir.join(self.service_type.as_str());
            if let Ok(metadata) = tokio::fs::metadata(&plugin).await {
                if metadata.is_file() {
                    return Ok(plugin);
                }
            }
        }

        let dirs: Vec<&str> = self.plugin_paths.iter().map(|dir| dir.as_str()).collect();
        Err(ServiceActionError::NotSupported(format!(
            "No service plugin for the service type '{}': no '{}' file in {}",
            self.service_type,
            self.service_type,
            dirs.join(", ")
        )))
    }
}

/// The failure of `execute_with_timeout`, which each caller reports with its own `ServiceActionError`
enum ExecutionError {
    NotStarted(std::io::Error),
    Terminated(Termination),
}

/// How the command of an action was terminated before it completed
#[derive(Debug, thiserror::Error)]
enum Termination {
    #[error(
        "did not complete within {} and was stopped with SIGTERM",
        humantime::format_duration(*.timeout)
    )]
    Timeout { timeout: Duration },

    #[error(
        "did not complete within {} and did not stop within {} after SIGTERM, so it was killed with SIGKILL",
        humantime::format_duration(*.timeout),
        humantime::format_duration(*.forceful_timeout)
    )]
    KilledOnTimeout {
        timeout: Duration,
        forceful_timeout: Duration,
    },

    #[error("did not complete and could not be stopped")]
    NotStopped,

    #[error("was interrupted")]
    Interrupted,
}

#[derive(Debug, thiserror::Error)]
enum ServiceActionError {
    #[error("{0}")]
    NotSupported(String),

    #[error(transparent)]
    InitSystem(#[from] SystemServiceError),

    #[error("The init system command <{service_command}> failed with exit code {code}")]
    InitSystemFailed { service_command: String, code: i32 },

    #[error("The init system command <{service_command}> was killed by a signal")]
    InitSystemKilled { service_command: String },

    #[error("The init system command <{service_command}> {termination}")]
    InitSystemTerminated {
        service_command: String,
        termination: Termination,
    },

    #[error(transparent)]
    SystemToml(#[from] tedge_config::SystemTomlError),

    #[error("Failed to run the service plugin {plugin}")]
    PluginNotExecutable {
        plugin: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("The service plugin {plugin} failed with exit code {code}")]
    PluginFailed { plugin: Utf8PathBuf, code: i32 },

    #[error("The service plugin {plugin} was killed by a signal")]
    PluginKilled { plugin: Utf8PathBuf },

    #[error("The service plugin {plugin} {termination}")]
    PluginTerminated {
        plugin: Utf8PathBuf,
        termination: Termination,
    },
}

impl ServiceActionError {
    fn is_timeout(&self) -> bool {
        matches!(
            self,
            ServiceActionError::InitSystemTerminated { termination, .. }
            | ServiceActionError::PluginTerminated { termination, .. }
                if !matches!(termination, Termination::Interrupted)
        )
    }
}

/// Remove workflow markers from the output of a service action, if any, before forwarding it to the caller.
/// Otherwise, when a workflow calls tedge service, a marker could inject anything for the workflow.
fn without_workflow_markers(stdout: &str) -> String {
    let holds_a_marker =
        |text: &str| text.contains(BEGIN_TEDGE_MARKER) || text.contains(END_TEDGE_MARKER);

    if !holds_a_marker(stdout) {
        return stdout.to_string();
    }

    stdout
        .lines()
        .filter(|line| !holds_a_marker(line))
        .map(|line| format!("{line}\n"))
        .collect()
}

fn forward_output(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{}", without_workflow_markers(&stdout));
    eprint!("{stderr}");
}

/// Only the actions of the default service type are completed, as defined in system.toml.
/// A completer cannot read the `--type` argument, so it cannot tell which service type is used.
/// Even when the service type is known, a service plugin cannot be asked which actions it supports.
fn action_completions() -> Vec<CompletionCandidate> {
    let Ok(config_dir) = Utf8PathBuf::try_from(get_config_dir()) else {
        return vec![];
    };
    action_completions_for_config_dir(&config_dir)
}

fn action_completions_for_config_dir(config_dir: &Utf8Path) -> Vec<CompletionCandidate> {
    let Ok(config) = SystemConfig::try_new(config_dir) else {
        return vec![];
    };
    config
        .init
        .action_names()
        .into_iter()
        .map(CompletionCandidate::new)
        .collect()
}

fn service_type_completions() -> Vec<CompletionCandidate> {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current()
            .block_on(service_type_completions_for_config_dir(get_config_dir()))
    })
}

async fn service_type_completions_for_config_dir(
    config_dir: impl AsRef<Path>,
) -> Vec<CompletionCandidate> {
    let mut types = BTreeSet::from([DEFAULT_SERVICE_TYPE.to_string()]);
    if let Ok(config) = TEdgeConfig::load(config_dir).await {
        for dir in &config.service.plugin_paths.0 {
            let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
                continue;
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let is_file = tokio::fs::metadata(entry.path())
                    .await
                    .is_ok_and(|metadata| metadata.is_file());
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_file && name.parse::<ServiceType>().is_ok() {
                    types.insert(name);
                }
            }
        }
    }
    types.into_iter().map(CompletionCandidate::new).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use tedge_api::workflow::extract_script_output;
    use tedge_test_utils::fs::with_exec_permission;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[tokio::test]
    async fn a_custom_type_runs_its_plugin() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.path();
        service_plugin(plugin_dir, "container");
        let config = TEdgeConfig::load_toml_str_with_root_dir(dir.path(), "");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        assert!(cmd.run(&config).await.is_ok());
    }

    #[tokio::test]
    async fn a_plugin_exiting_with_2_reports_the_action_as_not_supported() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.path();
        service_plugin(plugin_dir, "container");

        let mut cmd = command("reload", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert_eq!(
            err.to_string(),
            "The 'reload' action is not supported for the service type 'container'"
        );
    }

    #[tokio::test]
    async fn a_failing_plugin_is_reported_with_its_exit_code() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.path();
        let plugin = service_plugin(plugin_dir, "container");

        let mut cmd = command("stop", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::PluginFailed { .. });
        assert_eq!(
            err.to_string(),
            format!("The service plugin {plugin} failed with exit code 1")
        );
    }

    #[tokio::test]
    async fn an_init_system_failure_is_reported_with_its_exit_code() {
        let config_dir = TempTedgeDir::new();
        config_dir.file("system.toml").with_raw_content(
            r#"[init]
name = "test"
is_available = ["/bin/true"]
is_active = ["/bin/sh", "-c", "echo {} is inactive; exit 3"]
restart = ["/bin/true", "{}"]
stop = ["/bin/true", "{}"]
enable = ["/bin/true", "{}"]
disable = ["/bin/true", "{}"]
"#,
        );

        let cmd = command("is_active", "nginx", "service");
        let err = cmd
            .run_init_system_action(config_dir.path())
            .await
            .unwrap_err();

        assert!(
            err.to_string().ends_with("failed with exit code 3"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_plugin_not_completing_in_time_is_terminated() {
        let dir = TempTedgeDir::new();
        let plugin = dir.path().join("container");
        with_exec_permission(&plugin, "#!/bin/sh\nsleep 60\n");

        let mut cmd = command("start", "nodered", "container");
        cmd.plugin_paths = vec![dir.path().to_path_buf()];
        cmd.graceful_timeout = Duration::from_millis(100);
        cmd.forceful_timeout = Duration::from_secs(1);

        let err = tokio::time::timeout(Duration::from_secs(10), cmd.run_plugin_action())
            .await
            .expect("the plugin to be terminated on timeout")
            .unwrap_err();

        assert_matches!(err, ServiceActionError::PluginTerminated { .. });
        assert_eq!(
            err.to_string(),
            format!(
                "The service plugin {plugin} did not complete within 100ms and was stopped with SIGTERM"
            )
        );
    }

    #[tokio::test]
    async fn a_plugin_ignoring_sigterm_is_not_reported_as_stopped_with_sigterm() {
        let dir = TempTedgeDir::new();
        let plugin = dir.path().join("container");
        with_exec_permission(&plugin, "#!/bin/sh\ntrap '' TERM\nsleep 60\n");

        let mut cmd = command("start", "nodered", "container");
        cmd.plugin_paths = vec![dir.path().to_path_buf()];
        cmd.graceful_timeout = Duration::from_secs(1);
        cmd.forceful_timeout = Duration::from_millis(200);

        let err = tokio::time::timeout(Duration::from_secs(10), cmd.run_plugin_action())
            .await
            .expect("the plugin to be killed on timeout")
            .unwrap_err();

        assert_matches!(err, ServiceActionError::PluginTerminated { .. });
        assert_eq!(
            err.to_string(),
            format!(
                "The service plugin {plugin} did not complete within 1s and did not stop within 200ms after SIGTERM, so it was killed with SIGKILL"
            )
        );
    }

    #[tokio::test]
    async fn an_init_system_command_not_completing_in_time_is_terminated() {
        let config_dir = TempTedgeDir::new();
        config_dir.file("system.toml").with_raw_content(
            r#"[init]
name = "test"
is_available = ["/bin/true"]
start = ["/bin/sh", "-c", "sleep 60", "{}"]
stop = ["/bin/true", "{}"]
restart = ["/bin/true", "{}"]
enable = ["/bin/true", "{}"]
disable = ["/bin/true", "{}"]
is_active = ["/bin/true", "{}"]
"#,
        );

        let mut cmd = command("start", "nginx", "service");
        cmd.graceful_timeout = Duration::from_millis(100);
        cmd.forceful_timeout = Duration::from_secs(1);

        let err = tokio::time::timeout(
            Duration::from_secs(10),
            cmd.run_init_system_action(config_dir.path()),
        )
        .await
        .expect("the init system command to be terminated on timeout")
        .unwrap_err();

        assert_matches!(err, ServiceActionError::InitSystemTerminated { .. });
        assert_eq!(
            err.to_string(),
            "The init system command </bin/sh -c sleep 60 nginx> did not complete within 100ms and was stopped with SIGTERM"
        );
    }

    #[test_case(Termination::Timeout { timeout: Duration::from_secs(1) })]
    #[test_case(Termination::KilledOnTimeout { timeout: Duration::from_secs(1), forceful_timeout: Duration::from_secs(1) })]
    #[test_case(Termination::NotStopped)]
    fn an_action_not_completing_in_time_exits_with_the_timeout_exit_code(termination: Termination) {
        let err = ServiceActionError::PluginTerminated {
            plugin: "/usr/share/tedge/service-plugins/container".into(),
            termination,
        };

        assert!(err.is_timeout());
    }

    #[test]
    fn an_interrupted_action_does_not_exit_with_the_timeout_exit_code() {
        let err = ServiceActionError::PluginTerminated {
            plugin: "/usr/share/tedge/service-plugins/container".into(),
            termination: Termination::Interrupted,
        };

        assert!(!err.is_timeout());
    }

    #[tokio::test]
    async fn the_default_type_runs_the_action_through_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);
        let config = TEdgeConfig::load_toml_str_with_root_dir(config_dir.path(), "");

        let cmd = command("restart", "collectd", "service");
        cmd.run(&config).await.unwrap();

        assert!(Utf8PathBuf::from(format!("{done}.restart.collectd")).exists());
    }

    #[tokio::test]
    async fn a_custom_action_template_is_run_by_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);

        let cmd = command("reload", "nginx", "service");
        cmd.run_init_system_action(config_dir.path()).await.unwrap();

        assert!(Utf8PathBuf::from(format!("{done}.reload.nginx")).exists());
    }

    #[tokio::test]
    async fn an_action_with_no_template_is_reported_as_not_supported() {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        let cmd = command("pause", "collectd", "service");
        let err = cmd
            .run_init_system_action(config_dir.path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert!(
            err.to_string().contains(
                "Defined actions: disable, enable, is_active, reload, restart, start, stop"
            ),
            "{err}"
        );
    }

    #[test_case("name")]
    #[test_case("is_available")]
    #[tokio::test]
    async fn a_key_describing_the_init_system_is_not_a_service_action(key: &str) {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        let cmd = command(key, "nginx", "service");
        let err = cmd
            .run_init_system_action(config_dir.path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert_eq!(
            err.to_string(),
            format!(
                "'{key}' is a reserved key in system.toml that describes the init system \
                itself, not a service action.\nDefined actions: disable, enable, is_active, \
                reload, restart, start, stop."
            )
        );
    }

    #[tokio::test]
    async fn a_missing_plugin_is_reported_as_not_supported() {
        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec!["/no/such/directory".into(), "/nor/this/one".into()];

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert_eq!(
            err.to_string(),
            "No service plugin for the service type 'container': no 'container' file in /no/such/directory, /nor/this/one"
        );
    }

    #[tokio::test]
    async fn the_plugin_of_the_first_directory_holding_one_is_used() {
        let dir = TempTedgeDir::new();
        let first = dir.dir("first");
        let second = dir.dir("second");
        let plugin = service_plugin(first.path(), "container");
        service_plugin(second.path(), "container");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec![first.path().to_path_buf(), second.path().to_path_buf()];

        assert_eq!(cmd.find_plugin().await.unwrap(), plugin);
    }

    #[test]
    fn the_workflow_markers_are_not_forwarded_to_the_caller() {
        let stdout = "restarting nodered\n\
                      :::begin-tedge:::\n\
                      {\"logPath\": \"/etc/tedge/device-certs/tedge-private.pem\"}\n\
                      :::end-tedge:::\n";

        assert_eq!(
            without_workflow_markers(stdout),
            "restarting nodered\n{\"logPath\": \"/etc/tedge/device-certs/tedge-private.pem\"}\n"
        );

        assert_eq!(
            extract_script_output(without_workflow_markers(stdout)),
            None
        );
    }

    #[test]
    fn the_actions_defined_in_system_toml_are_completed() {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        let actions: Vec<_> = action_completions_for_config_dir(config_dir.path())
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            actions,
            [
                "disable",
                "enable",
                "is_active",
                "reload",
                "restart",
                "start",
                "stop"
            ]
        );
    }

    #[test]
    fn the_standard_actions_are_completed_when_there_is_no_system_toml() {
        let config_dir = TempTedgeDir::new();

        let actions: Vec<_> = action_completions_for_config_dir(config_dir.path())
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect();

        assert_eq!(
            actions,
            ["disable", "enable", "is_active", "restart", "start", "stop"]
        );
    }

    #[tokio::test]
    async fn the_service_types_completed_are_the_default_type_and_the_plugin_files() {
        let config_dir = TempTedgeDir::new();
        let first = config_dir.dir("first");
        let second = config_dir.dir("second");
        service_plugin(first.path(), "container");
        service_plugin(second.path(), "container");
        service_plugin(second.path(), "lxc");
        second.dir("not-a-plugin");
        config_dir.file("tedge.toml").with_raw_content(&format!(
            "service.plugin_paths = [\"{}\", \"{}\", \"/no/such/directory\"]",
            first.path(),
            second.path()
        ));

        let types: Vec<_> = service_type_completions_for_config_dir(config_dir.path())
            .await
            .iter()
            .map(|candidate| candidate.get_value().to_string_lossy().into_owned())
            .collect();

        assert_eq!(types, ["container", "lxc", "service"]);
    }

    fn command(action: &str, service_name: &str, service_type: &str) -> ServiceActionCommand {
        ServiceActionCommand {
            action: action.parse().unwrap(),
            service_name: service_name.parse().unwrap(),
            service_type: service_type.parse().unwrap(),
            plugin_paths: vec![],
            graceful_timeout: Duration::from_secs(10),
            forceful_timeout: Duration::from_secs(10),
        }
    }

    fn service_plugin(dir: &Utf8Path, service_type: &str) -> Utf8PathBuf {
        let plugin = dir.join(service_type);
        with_exec_permission(
            &plugin,
            r#"#!/bin/sh
echo "called with: $*"
case "$1" in
  restart) exit 0;;
  stop) echo "starting $2" >&2; echo "$1 failed on $2" >&2; exit 1;;
  *) echo "$1 is not implemented" >&2; exit 2;;
esac
"#,
        );
        plugin
    }

    fn init_system(config_dir: &TempTedgeDir) -> Utf8PathBuf {
        // Each action leaves a file named after the action and the service it was run on
        let done = config_dir.path().join("done");
        config_dir.file("system.toml").with_raw_content(&format!(
            r#"[init]
name = "test"
is_available = ["/usr/bin/touch", "{done}.is_available"]
is_active = ["/usr/bin/touch", "{done}.is_active.{{}}"]
start = ["/usr/bin/touch", "{done}.start.{{}}"]
stop = ["/usr/bin/touch", "{done}.stop.{{}}"]
restart = ["/usr/bin/touch", "{done}.restart.{{}}"]
enable = ["/usr/bin/touch", "{done}.enable.{{}}"]
disable = ["/usr/bin/touch", "{done}.disable.{{}}"]
reload = ["/usr/bin/touch", "{done}.reload.{{}}"]
"#
        ));
        done
    }
}
