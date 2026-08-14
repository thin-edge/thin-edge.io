use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::process::Stdio;
use tedge_api::service_command::validate_action_name;
use tedge_api::service_command::validate_service_name;
use tedge_api::service_command::validate_service_type;
use tedge_api::service_command::InvalidActionName;
use tedge_api::service_command::InvalidServiceName;
use tedge_api::service_command::InvalidServiceType;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::workflow::BEGIN_TEDGE_MARKER;
use tedge_api::workflow::END_TEDGE_MARKER;
use tedge_config::TEdgeConfig;
use tedge_system_services::failure_reason;
use tedge_system_services::service_manager;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceError;

/// The exit code of an action that is not supported for the service type
const NOT_SUPPORTED_EXIT_CODE: i32 = 2;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeServiceOpt {
    /// The action to run on the service, e.g. start, stop or restart
    ///
    /// Which actions are supported is decided depending on the service type.
    /// Init system for the default service type, a service plugin for any other type.
    action: String,

    /// The name of the service to act on
    service_name: String,

    /// The type of the service
    ///
    /// The default type is handled by the init system configured in system.toml.
    /// Any other type is handled by the service plugin.
    #[clap(long, default_value = DEFAULT_SERVICE_TYPE)]
    service_type: String,
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
        }
        .into_boxed())
    }
}

#[derive(Debug)]
pub struct ServiceActionCommand {
    action: String,
    service_name: String,
    service_type: String,
    plugin_paths: Vec<Utf8PathBuf>,
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
            // Exits here, as this outcome has an exit code of its own
            Err(ServiceActionError::NotSupported(reason)) => {
                eprintln!("Error: {reason}");
                std::process::exit(NOT_SUPPORTED_EXIT_CODE)
            }
            Err(err) => Err(MaybeFancy::Unfancy(err.into())),
        }
    }
}

impl ServiceActionCommand {
    async fn run(&self, config: &TEdgeConfig) -> Result<(), ServiceActionError> {
        self.validate()?;

        if self.service_type == DEFAULT_SERVICE_TYPE {
            self.run_init_system_action(config.root_dir()).await
        } else {
            self.run_plugin_action().await
        }
    }

    /// Check every argument before use, since this process runs as root
    fn validate(&self) -> Result<(), ServiceActionError> {
        validate_action_name(&self.action)?;
        validate_service_name(&self.service_name)?;
        validate_service_type(&self.service_type)?;
        Ok(())
    }

    /// Run the action through the init system abstraction configured in system.toml
    async fn run_init_system_action(
        &self,
        config_root: &Utf8Path,
    ) -> Result<(), ServiceActionError> {
        let service_manager = service_manager(config_root)?;
        let service = SystemService::new(&self.service_name);

        let output = service_manager
            .run_action(&self.action, service)
            .await
            .map_err(|err| match err {
                err @ SystemServiceError::UnsupportedAction { .. } => {
                    ServiceActionError::NotSupported(err.to_string())
                }
                err => err.into(),
            })?;

        forward(&output.stdout, &output.stderr);
        Ok(())
    }

    /// Run the action through the service plugin whose file name is the service type
    async fn run_plugin_action(&self) -> Result<(), ServiceActionError> {
        let plugin = self.find_plugin()?;

        let output = tokio::process::Command::new(&plugin)
            .arg(&self.action)
            .arg(&self.service_name)
            .stdin(Stdio::null())
            .output()
            .await;

        let output = match output {
            Ok(output) => output,
            Err(err) => {
                return Err(ServiceActionError::PluginNotExecutable {
                    plugin,
                    source: err,
                })
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        forward(&stdout, &stderr);

        match output.status.code() {
            Some(0) => Ok(()),
            Some(code) if code == NOT_SUPPORTED_EXIT_CODE => {
                Err(ServiceActionError::NotSupported(format!(
                    "The '{}' action is not supported for the service type '{}'",
                    self.action, self.service_type
                )))
            }
            Some(code) => Err(ServiceActionError::PluginFailed {
                plugin,
                code,
                reason: failure_reason(&stderr),
            }),
            None => Err(ServiceActionError::PluginKilled { plugin }),
        }
    }

    /// The plugin file whose name is the service type,
    /// taken from the first configured directory that holds one
    fn find_plugin(&self) -> Result<Utf8PathBuf, ServiceActionError> {
        self.plugin_paths
            .iter()
            .map(|dir| dir.join(&self.service_type))
            .find(|plugin| plugin.is_file())
            .ok_or_else(|| {
                let dirs: Vec<&str> = self.plugin_paths.iter().map(|dir| dir.as_str()).collect();
                ServiceActionError::NotSupported(format!(
                    "No service plugin for the service type '{}': no '{}' file in {}",
                    self.service_type,
                    self.service_type,
                    dirs.join(", ")
                ))
            })
    }
}

#[derive(Debug, thiserror::Error)]
enum ServiceActionError {
    #[error("{0}")]
    NotSupported(String),

    #[error(transparent)]
    InvalidActionName(#[from] InvalidActionName),

    #[error(transparent)]
    InvalidServiceName(#[from] InvalidServiceName),

    #[error(transparent)]
    InvalidServiceType(#[from] InvalidServiceType),

    #[error(transparent)]
    InitSystem(#[from] SystemServiceError),

    #[error(transparent)]
    SystemToml(#[from] tedge_config::SystemTomlError),

    #[error("Failed to run the service plugin {plugin}")]
    PluginNotExecutable {
        plugin: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("The service plugin {plugin} failed with exit code {code}: {reason}")]
    PluginFailed {
        plugin: Utf8PathBuf,
        code: i32,
        reason: String,
    },

    #[error("The service plugin {plugin} was killed by a signal")]
    PluginKilled { plugin: Utf8PathBuf },
}

/// The action's stdout, with every line holding a workflow marker removed
///
/// When tedge-agent runs this process as a workflow script, a marker there would make the
/// free-form output of an action update the state of the command.
fn without_workflow_markers(stdout: &str) -> String {
    let holds_a_marker =
        |text: &str| text.contains(BEGIN_TEDGE_MARKER) || text.contains(END_TEDGE_MARKER);

    // Nothing to strip: the output is forwarded byte for byte, trailing newline included
    if !holds_a_marker(stdout) {
        return stdout.to_string();
    }

    stdout
        .lines()
        .filter(|line| !holds_a_marker(line))
        .map(|line| format!("{line}\n"))
        .collect()
}

fn forward(stdout: &str, stderr: &str) {
    print!("{}", without_workflow_markers(stdout));
    eprint!("{stderr}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use tedge_api::workflow::extract_script_output;
    use tedge_test_utils::fs::with_exec_permission;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn a_custom_type_runs_its_plugin() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
        service_plugin(plugin_dir, "container");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        assert!(cmd.run_plugin_action().await.is_ok());
    }

    #[tokio::test]
    async fn a_plugin_exiting_with_2_reports_the_action_as_not_supported() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
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
    async fn a_failing_plugin_is_reported_with_its_last_stderr_line() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
        let plugin = service_plugin(plugin_dir, "container");

        let mut cmd = command("stop", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::PluginFailed { .. });
        assert_eq!(
            err.to_string(),
            format!("The service plugin {plugin} failed with exit code 1: stop failed on nodered")
        );
    }

    #[tokio::test]
    async fn the_default_type_runs_the_action_through_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);

        let cmd = command("restart", "collectd", "service");
        cmd.run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap();

        assert!(Utf8PathBuf::from(format!("{done}.restart.collectd")).exists());
    }

    #[tokio::test]
    async fn a_custom_action_template_is_run_by_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);

        let cmd = command("reload", "nginx", "service");
        cmd.run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap();

        assert!(Utf8PathBuf::from(format!("{done}.reload.nginx")).exists());
    }

    #[tokio::test]
    async fn an_action_with_no_template_is_reported_as_not_supported() {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        let cmd = command("pause", "collectd", "service");
        let err = cmd
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        // The error tells which actions this init system does define
        assert!(
            err.to_string().contains(
                "Defined actions: disable, enable, is_active, reload, restart, start, stop"
            ),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_state_query_template_is_run_by_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);

        let cmd = command("is_active", "nginx", "service");
        cmd.run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap();

        assert!(Utf8PathBuf::from(format!("{done}.is_active.nginx")).exists());
    }

    #[tokio::test]
    async fn is_available_is_not_a_service_action() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);

        let cmd = command("is_available", "nginx", "service");
        let err = cmd
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert!(!Utf8PathBuf::from(format!("{done}.is_available")).exists());
    }

    #[tokio::test]
    async fn a_state_query_answering_no_is_a_failure_not_an_unsupported_action() {
        let config_dir = TempTedgeDir::new();
        config_dir.file("system.toml").with_raw_content(
            r#"[init]
name = "test"
is_available = ["/bin/true"]
is_active = ["/bin/false", "{}"]
restart = ["/bin/true", "{}"]
stop = ["/bin/true", "{}"]
enable = ["/bin/true", "{}"]
disable = ["/bin/true", "{}"]
"#,
        );

        let cmd = command("is_active", "nginx", "service");
        let err = cmd
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert_matches!(
            err,
            ServiceActionError::InitSystem(SystemServiceError::ServiceCommandFailedWithCode {
                code: 1,
                ..
            })
        );
    }

    #[tokio::test]
    async fn the_reason_of_an_init_system_failure_comes_from_its_stderr() {
        let config_dir = TempTedgeDir::new();
        config_dir.file("system.toml").with_raw_content(
            r#"[init]
name = "test"
is_available = ["/bin/true"]
is_active = ["/bin/true", "{}"]
restart = ["/bin/sh", "-c", "echo Unit {}.service not found. >&2; exit 5"]
stop = ["/bin/true", "{}"]
enable = ["/bin/true", "{}"]
disable = ["/bin/true", "{}"]
"#,
        );

        let cmd = command("restart", "nginx", "service");
        let err = cmd
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("Unit nginx.service not found."),
            "{err}"
        );
    }

    #[tokio::test]
    async fn the_name_key_is_not_dispatchable_as_an_action() {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        let cmd = command("name", "collectd", "service");
        let err = cmd
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
    }

    #[tokio::test]
    async fn a_missing_plugin_is_reported_as_not_supported() {
        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec!["/no/such/directory".into(), "/nor/this/one".into()];

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        // The error names every directory that was searched
        assert_eq!(
            err.to_string(),
            "No service plugin for the service type 'container': no 'container' file in /no/such/directory, /nor/this/one"
        );
    }

    #[test]
    fn the_plugin_of_the_first_directory_holding_one_is_used() {
        let dir = TempTedgeDir::new();
        let first = dir.dir("first");
        let second = dir.dir("second");
        let plugin = service_plugin(first.utf8_path(), "container");
        service_plugin(second.utf8_path(), "container");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec![
            first.utf8_path().to_path_buf(),
            second.utf8_path().to_path_buf(),
        ];

        assert_eq!(cmd.find_plugin().unwrap(), plugin);
    }

    #[test]
    fn a_directory_holding_no_such_plugin_is_skipped() {
        let dir = TempTedgeDir::new();
        let plugin = service_plugin(dir.utf8_path(), "container");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec!["/no/such/directory".into(), dir.utf8_path().to_path_buf()];

        assert_eq!(cmd.find_plugin().unwrap(), plugin);
    }

    #[test]
    fn the_workflow_markers_are_not_forwarded_to_the_caller() {
        // tedge-agent would read such a block as a state update for the command it is running
        let stdout = "restarting nodered\n\
                      :::begin-tedge:::\n\
                      {\"logPath\": \"/etc/tedge/device-certs/tedge-private.pem\"}\n\
                      :::end-tedge:::\n";

        assert_eq!(
            without_workflow_markers(stdout),
            "restarting nodered\n{\"logPath\": \"/etc/tedge/device-certs/tedge-private.pem\"}\n"
        );
        // The remaining excerpt is no longer read as a state update
        assert_eq!(
            extract_script_output(without_workflow_markers(stdout)),
            None
        );
    }

    #[test]
    fn an_argument_is_validated_before_any_backend_is_selected() {
        assert_matches!(
            command("RESTART", "collectd", "service").validate(),
            Err(ServiceActionError::InvalidActionName(_))
        );
        assert_matches!(
            command("restart", "--now", "service").validate(),
            Err(ServiceActionError::InvalidServiceName(_))
        );
        assert_matches!(
            command("restart", "collectd", "../../bin/sh").validate(),
            Err(ServiceActionError::InvalidServiceType(_))
        );
        assert!(command("restart", "collectd", "container")
            .validate()
            .is_ok());
    }

    fn command(action: &str, service_name: &str, service_type: &str) -> ServiceActionCommand {
        ServiceActionCommand {
            action: action.to_string(),
            service_name: service_name.to_string(),
            service_type: service_type.to_string(),
            plugin_paths: vec!["/usr/share/tedge/service-plugins".into()],
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
        let done = config_dir.utf8_path().join("done");
        // Each action runs a command which leaves the action and the service name on disk
        let touch = |action: &str| format!(r#"["/usr/bin/touch", "{done}.{action}.{{}}"]"#);
        config_dir.file("system.toml").with_raw_content(&format!(
            r#"[init]
name = "test"
is_available = ["/usr/bin/touch", "{done}.is_available"]
is_active = {}
start = {}
stop = {}
restart = {}
enable = {}
disable = {}
reload = {}
"#,
            touch("is_active"),
            touch("start"),
            touch("stop"),
            touch("restart"),
            touch("enable"),
            touch("disable"),
            touch("reload"),
        ));
        done
    }
}
