use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::process::Stdio;
use tedge_api::service_command::validate_action_name;
use tedge_api::service_command::InvalidActionName;
use tedge_api::service_command::DEFAULT_SERVICE_TYPE;
use tedge_api::workflow::BEGIN_TEDGE_MARKER;
use tedge_api::workflow::END_TEDGE_MARKER;
use tedge_config::TEdgeConfig;
use tedge_system_services::service_manager;
use tedge_system_services::SystemService;
use tedge_system_services::SystemServiceError;

/// The service type handled by the init system, rather than by a service plugin.
const INIT_SERVICE_TYPE: &str = DEFAULT_SERVICE_TYPE;

/// The exit code telling the caller that the action is not supported for that service type,
/// as opposed to an action that was run and failed. Same meaning as for the diag plugins.
const NOT_SUPPORTED_EXIT_CODE: i32 = 2;

const MAX_SERVICE_NAME_LEN: usize = 128;
const MAX_SERVICE_TYPE_LEN: usize = 64;

#[derive(clap::Args, Debug, Eq, PartialEq)]
pub struct TEdgeServiceOpt {
    /// The action to run on the service, e.g. start, stop or restart
    ///
    /// Which actions are supported is decided by the backend running them:
    /// the init system for the default service type, a service plugin for any other type.
    action: String,

    /// The name of the service to act on, as the backend knows it
    service_name: String,

    /// The type of the service, selecting the backend that runs the action
    ///
    /// The default type is handled by the init system configured in system.toml.
    /// Any other type is handled by the service plugin named after it.
    #[clap(long, default_value = INIT_SERVICE_TYPE)]
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
            plugin_dir: config.service.plugin_dir.clone(),
        }
        .into_boxed())
    }
}

#[derive(Debug)]
pub struct ServiceActionCommand {
    action: String,
    service_name: String,
    service_type: String,
    plugin_dir: Utf8PathBuf,
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
            // Reported here rather than returned, as this outcome has its own exit code
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

        if self.service_type == INIT_SERVICE_TYPE {
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

        service_manager
            .run_action(&self.action, service)
            .await
            .map_err(|err| match err {
                // The message lists the actions the init system does define
                err @ SystemServiceError::UnsupportedAction { .. } => {
                    ServiceActionError::NotSupported(err.to_string())
                }
                err => err.into(),
            })
    }

    /// Run the action through the service plugin named after the service type
    async fn run_plugin_action(&self) -> Result<(), ServiceActionError> {
        let plugin = self.plugin_dir.join(&self.service_type);

        // Execution is argv-based: no argument can become a shell fragment
        let output = tokio::process::Command::new(&plugin)
            .arg(&self.action)
            .arg(&self.service_name)
            .stdin(Stdio::null())
            .output()
            .await;

        let output = match output {
            Ok(output) => output,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ServiceActionError::NotSupported(format!(
                    "No service plugin for the service type '{}': {plugin} does not exist",
                    self.service_type
                )))
            }
            Err(err) => {
                return Err(ServiceActionError::PluginNotExecutable {
                    plugin,
                    source: err,
                })
            }
        };

        // The plugin's own output belongs to the caller, and to the workflow log when
        // the caller is tedge-agent
        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{}", without_workflow_markers(&stdout));
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprint!("{stderr}");

        match output.status.code() {
            Some(0) => Ok(()),
            // The plugin's own "not supported", propagated unchanged
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
}

#[derive(Debug, thiserror::Error)]
enum ServiceActionError {
    /// Not an error of the action itself: the backend cannot run it at all
    #[error("{0}")]
    NotSupported(String),

    #[error(transparent)]
    InvalidActionName(#[from] InvalidActionName),

    #[error("Invalid service name '{name}': {reason}")]
    InvalidServiceName { name: String, reason: String },

    #[error("Invalid service type '{ty}': {reason}")]
    InvalidServiceType { ty: String, reason: String },

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

/// The plugin's stdout, with every line holding a workflow marker removed
///
/// When the caller is tedge-agent, the stdout of this process is the stdout of a workflow script,
/// and a JSON excerpt surrounded by the workflow markers there updates the state of the command.
/// A plugin prints free-form text, not state updates, so such a line is dropped rather than
/// forwarded. A marker is looked for anywhere in a line, as the workflow engine does.
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

/// The last thing the plugin said on stderr, used as the reason of the failure
fn failure_reason(stderr: &str) -> String {
    match stderr.lines().rfind(|line| !line.trim().is_empty()) {
        Some(reason) => reason.trim().to_string(),
        None => "no reason given".to_string(),
    }
}

/// A service name is passed to the init tool or to a plugin, so it must not be read as an option
fn validate_service_name(name: &str) -> Result<(), ServiceActionError> {
    let invalid = |reason: &str| {
        Err(ServiceActionError::InvalidServiceName {
            name: name.to_string(),
            reason: reason.to_string(),
        })
    };

    if name.is_empty() {
        return invalid("a service name cannot be empty");
    }
    if name.len() > MAX_SERVICE_NAME_LEN {
        return invalid(&format!(
            "a service name cannot be longer than {MAX_SERVICE_NAME_LEN} characters"
        ));
    }
    if name.starts_with('-') {
        return invalid("a service name cannot start with '-'");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '@' | '-'))
    {
        return invalid("a service name only holds letters, digits, '_', '.', '@' and '-'");
    }

    Ok(())
}

/// A service type selects a file in the plugin directory, so it must not allow path traversal
fn validate_service_type(ty: &str) -> Result<(), ServiceActionError> {
    let invalid = |reason: &str| {
        Err(ServiceActionError::InvalidServiceType {
            ty: ty.to_string(),
            reason: reason.to_string(),
        })
    };

    if ty.is_empty() {
        return invalid("a service type cannot be empty");
    }
    if ty.len() > MAX_SERVICE_TYPE_LEN {
        return invalid(&format!(
            "a service type cannot be longer than {MAX_SERVICE_TYPE_LEN} characters"
        ));
    }
    if !ty
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid("a service type only holds lowercase letters, digits, '_' and '-'");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::os::unix::fs::PermissionsExt;
    use tedge_api::workflow::extract_script_output;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    fn command(action: &str, service_name: &str, service_type: &str) -> ServiceActionCommand {
        ServiceActionCommand {
            action: action.to_string(),
            service_name: service_name.to_string(),
            service_type: service_type.to_string(),
            plugin_dir: "/usr/share/tedge/service-plugins".into(),
        }
    }

    #[test_case("collectd")]
    #[test_case("c8y-firmware-plugin")]
    #[test_case("getty@tty1")]
    #[test_case("my.service")]
    fn accepted_service_names(name: &str) {
        assert!(validate_service_name(name).is_ok(), "{name}");
    }

    #[test_case(""; "empty")]
    #[test_case("--now"; "looking like an option")]
    #[test_case("collectd stop"; "with a space")]
    #[test_case("collectd;reboot"; "with a shell separator")]
    #[test_case("../collectd"; "with a path")]
    fn rejected_service_names(name: &str) {
        assert_matches!(
            validate_service_name(name),
            Err(ServiceActionError::InvalidServiceName { .. }),
            "{name}"
        );
    }

    #[test_case("service")]
    #[test_case("container")]
    #[test_case("my_type-2")]
    fn accepted_service_types(ty: &str) {
        assert!(validate_service_type(ty).is_ok(), "{ty}");
    }

    #[test_case(""; "empty")]
    #[test_case("Container"; "with an uppercase letter")]
    #[test_case("../../bin/sh"; "with a path traversal")]
    #[test_case("sub/type"; "with a path separator")]
    #[test_case(".."; "parent directory")]
    fn rejected_service_types(ty: &str) {
        assert_matches!(
            validate_service_type(ty),
            Err(ServiceActionError::InvalidServiceType { .. }),
            "{ty}"
        );
    }

    /// A plugin reporting what it was called with, and exiting as the action asks
    fn service_plugin(dir: &Utf8Path, service_type: &str) -> Utf8PathBuf {
        let plugin = dir.join(service_type);
        std::fs::write(
            &plugin,
            r#"#!/bin/sh
echo "called with: $*"
case "$1" in
  restart) exit 0;;
  reload) echo "reload is not implemented" >&2; exit 2;;
  *) echo "starting $2" >&2; echo "$1 failed on $2" >&2; exit 1;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&plugin, PermissionsExt::from_mode(0o755)).unwrap();
        plugin
    }

    #[tokio::test]
    async fn a_custom_type_runs_its_plugin() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
        service_plugin(plugin_dir, "container");

        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_dir = plugin_dir.to_path_buf();

        assert!(cmd.run_plugin_action().await.is_ok());
    }

    #[tokio::test]
    async fn a_plugin_exiting_with_2_reports_the_action_as_not_supported() {
        let dir = TempTedgeDir::new();
        let plugin_dir = dir.utf8_path();
        service_plugin(plugin_dir, "container");

        let mut cmd = command("reload", "nodered", "container");
        cmd.plugin_dir = plugin_dir.to_path_buf();

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
        cmd.plugin_dir = plugin_dir.to_path_buf();

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::PluginFailed { .. });
        assert_eq!(
            err.to_string(),
            format!("The service plugin {plugin} failed with exit code 1: stop failed on nodered")
        );
    }

    /// An init system whose actions all write a file named after the action and the service,
    /// so that a test can tell which template was run
    fn init_system(config_dir: &TempTedgeDir) -> Utf8PathBuf {
        let done = config_dir.utf8_path().join("done");
        let touch = |action: &str| format!(r#"["/usr/bin/touch", "{done}.{action}.{{}}"]"#);
        config_dir.file("system.toml").with_raw_content(&format!(
            r#"[init]
name = "test"
is_available = ["/bin/true"]
is_active = ["/bin/true"]
start = {}
stop = {}
restart = {}
enable = {}
disable = {}
reload = {}
"#,
            touch("start"),
            touch("stop"),
            touch("restart"),
            touch("enable"),
            touch("disable"),
            touch("reload"),
        ));
        done
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
            err.to_string()
                .contains("Known actions: disable, enable, reload, restart, start, stop"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn a_reserved_key_is_not_dispatchable_as_an_action() {
        let config_dir = TempTedgeDir::new();
        init_system(&config_dir);

        for reserved in ["is_active", "is_available", "name"] {
            let cmd = command(reserved, "collectd", "service");
            let err = cmd
                .run_init_system_action(config_dir.utf8_path())
                .await
                .unwrap_err();

            assert_matches!(err, ServiceActionError::NotSupported(_), "{reserved}");
        }
    }

    #[tokio::test]
    async fn a_missing_plugin_is_reported_as_not_supported() {
        let mut cmd = command("restart", "nodered", "container");
        cmd.plugin_dir = "/no/such/directory".into();

        let err = cmd.run_plugin_action().await.unwrap_err();

        assert_matches!(err, ServiceActionError::NotSupported(_));
        assert_eq!(
            err.to_string(),
            "No service plugin for the service type 'container': /no/such/directory/container does not exist"
        );
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
    fn a_marker_is_stripped_wherever_it_is_on_the_line() {
        // The engine looks for the marker anywhere in the output, not only at the start of a line
        assert_eq!(
            without_workflow_markers("done :::begin-tedge:::\n{}\nand :::end-tedge:::\n"),
            "{}\n"
        );
    }

    #[test]
    fn any_other_output_of_a_plugin_is_forwarded_unchanged() {
        assert_eq!(
            without_workflow_markers("restarting nodered\n"),
            "restarting nodered\n"
        );
        // Down to a last line with no newline of its own
        assert_eq!(without_workflow_markers("no newline"), "no newline");
        assert_eq!(without_workflow_markers(""), "");
    }

    #[test]
    fn the_reason_of_a_failure_is_the_last_line_of_stderr() {
        assert_eq!(
            failure_reason("starting nodered\nno such container\n"),
            "no such container"
        );
        assert_eq!(failure_reason(""), "no reason given");
        assert_eq!(failure_reason("\n \n"), "no reason given");
    }

    #[test]
    fn an_argument_is_validated_before_any_backend_is_selected() {
        assert_matches!(
            command("RESTART", "collectd", "service").validate(),
            Err(ServiceActionError::InvalidActionName(_))
        );
        assert_matches!(
            command("restart", "--now", "service").validate(),
            Err(ServiceActionError::InvalidServiceName { .. })
        );
        assert_matches!(
            command("restart", "collectd", "../../bin/sh").validate(),
            Err(ServiceActionError::InvalidServiceType { .. })
        );
        assert!(command("restart", "collectd", "container")
            .validate()
            .is_ok());
    }
}
