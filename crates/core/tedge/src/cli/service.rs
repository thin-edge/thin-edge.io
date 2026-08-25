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
    #[clap(long("type"), default_value = DEFAULT_SERVICE_TYPE)]
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
        // Check every argument before use, since this process runs as root
        self.validate()?;

        if self.service_type == DEFAULT_SERVICE_TYPE {
            self.run_init_system_action(config.root_dir()).await
        } else {
            self.run_plugin_action().await
        }
    }

    fn validate(&self) -> Result<(), ServiceActionError> {
        validate_action_name(&self.action)?;
        validate_service_name(&self.service_name)?;
        validate_service_type(&self.service_type)?;
        Ok(())
    }

    async fn run_init_system_action(
        &self,
        config_root: &Utf8Path,
    ) -> Result<(), ServiceActionError> {
        let service_manager = service_manager(config_root)?;
        let service = SystemService::new(&self.service_name);

        let outcome = service_manager
            .run_action(&self.action, service)
            .await
            // Any error occurred before running a process
            .map_err(|err| match err {
                err @ (SystemServiceError::UnsupportedAction { .. }
                | SystemServiceError::NotAnAction { .. }) => {
                    ServiceActionError::NotSupported(err.to_string())
                }
                err => err.into(),
            })?;

        forward_output(&outcome.output.stdout, &outcome.output.stderr);

        if outcome.success() {
            return Ok(());
        }

        let service_command = outcome.service_command;
        match outcome.status.code() {
            Some(code) => Err(ServiceActionError::InitSystemFailed {
                service_command,
                code,
            }),
            None => Err(ServiceActionError::InitSystemKilled { service_command }),
        }
    }

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
        forward_output(&stdout, &stderr);

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

    #[error("The init system command <{service_command}> failed with exit code {code}")]
    InitSystemFailed { service_command: String, code: i32 },

    #[error("The init system command <{service_command}> was killed by a signal")]
    InitSystemKilled { service_command: String },

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

fn forward_output(stdout: &str, stderr: &str) {
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
    use test_case::test_case;

    #[tokio::test]
    async fn a_custom_type_runs_its_plugin() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
        service_plugin(plugin_dir, "container");
        let config = TEdgeConfig::load_toml_str_with_root_dir(dir.utf8_path(), "");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_paths = vec![plugin_dir.to_path_buf()];

        assert!(cmd.run(&config).await.is_ok());
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
    async fn a_failing_plugin_is_reported_with_its_exit_code() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
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
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert!(
            err.to_string().ends_with("failed with exit code 3"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn the_default_type_runs_the_action_through_the_init_system() {
        let config_dir = TempTedgeDir::new();
        let done = init_system(&config_dir);
        let config = TEdgeConfig::load_toml_str_with_root_dir(config_dir.utf8_path(), "");

        let cmd = command("restart", "collectd", "service");
        cmd.run(&config).await.unwrap();

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
            .run_init_system_action(config_dir.utf8_path())
            .await
            .unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert_eq!(
            err.to_string(),
            format!(
                "'{key}' is not a service action: the [init] table uses that key to describe the \
                init system.\nDefined actions: disable, enable, is_active, reload, restart, \
                start, stop."
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
        // Each action leaves a file named after the action and the service it was run on
        let done = config_dir.utf8_path().join("done");
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
