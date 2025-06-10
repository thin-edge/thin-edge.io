use crate::system_services::SystemService;
use crate::system_services::SystemServiceError;
use crate::system_services::SystemServiceManager;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::fmt;
use std::process::ExitStatus;
use std::process::Stdio;
use tedge_config::InitConfig;
use tedge_config::SystemConfig;
use tedge_config::SystemTomlError;
use tedge_config::SYSTEM_CONFIG_FILE;

#[derive(Debug)]
pub struct GeneralServiceManager {
    init_config: InitConfig,
    config_path: Utf8PathBuf,
}

impl GeneralServiceManager {
    pub fn try_new(config_root: &Utf8Path) -> Result<Self, SystemTomlError> {
        let init_config = SystemConfig::try_new(config_root)?.init;

        let config_path = config_root.join(SYSTEM_CONFIG_FILE);

        Ok(Self {
            init_config,
            config_path,
        })
    }
}

#[async_trait::async_trait]
impl SystemServiceManager for GeneralServiceManager {
    fn name(&self) -> &str {
        &self.init_config.name
    }

    async fn check_operational(&self) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::CheckManager.try_exec_command(self)?;

        match exec_command.to_command().status().await {
            Ok(status) if status.success() => Ok(()),
            _ => Err(SystemServiceError::ServiceManagerUnavailable {
                cmd: exec_command.to_string(),
                name: self.name().to_string(),
            }),
        }
    }

    async fn stop_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Stop(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await?
            .must_succeed()
    }

    async fn start_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Start(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await?
            .must_succeed()
    }

    async fn restart_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Restart(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await?
            .must_succeed()
    }

    async fn enable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Enable(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await?
            .must_succeed()
    }

    async fn disable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Disable(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await?
            .must_succeed()
    }

    async fn is_service_running(
        &self,
        service: SystemService<'_>,
    ) -> Result<bool, SystemServiceError> {
        let exec_command = ServiceCommand::IsActive(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
            .await
            .map(|status| status.success())
    }
}

#[derive(Debug, PartialEq)]
struct ExecCommand {
    exec: String,
    args: Vec<String>,
}

impl ExecCommand {
    fn try_new(
        config: Vec<String>,
        cmd: ServiceCommand,
        config_path: Utf8PathBuf,
    ) -> Result<Self, SystemServiceError> {
        match config.split_first() {
            Some((exec, args)) => Ok(Self {
                exec: exec.to_string(),
                args: args.to_vec(),
            }),
            None => Err(SystemServiceError::SystemConfigInvalidSyntax {
                reason: "Requires 1 or more arguments.".to_string(),
                cmd: cmd.to_string(),
                path: config_path,
            }),
        }
    }

    fn try_new_with_placeholder<'a>(
        config: Vec<String>,
        service_cmd: ServiceCommand<'a>,
        config_path: Utf8PathBuf,
        service: SystemService<'a>,
    ) -> Result<Self, SystemServiceError> {
        let replaced = replace_with_service_name(&config, service_cmd, &config_path, service)?;
        Self::try_new(replaced, service_cmd, config_path)
    }

    fn to_command(&self) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(&self.exec);
        cmd.args(&self.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }
}

impl fmt::Display for ExecCommand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.args.is_empty() {
            write!(f, "{}", self.exec)
        } else {
            let mut s = self.exec.to_owned();
            for arg in &self.args {
                s = format!("{} {}", s, arg);
            }
            write!(f, "{}", s)
        }
    }
}

fn replace_with_service_name<'a>(
    input_args: &[String],
    service_cmd: ServiceCommand<'a>,
    config_path: impl Into<Utf8PathBuf>,
    service: SystemService<'a>,
) -> Result<Vec<String>, SystemServiceError> {
    if !input_args.iter().any(|s| s == "{}") {
        return Err(SystemServiceError::SystemConfigInvalidSyntax {
            reason: "A placeholder '{}' is missing.".to_string(),
            cmd: service_cmd.to_string(),
            path: config_path.into(),
        });
    }

    let mut args = input_args.to_owned();
    for item in args.iter_mut() {
        if item == "{}" {
            *item = service.to_string();
        }
    }

    Ok(args)
}

#[derive(Debug, Copy, Clone)]
enum ServiceCommand<'a> {
    CheckManager,
    Stop(SystemService<'a>),
    Start(SystemService<'a>),
    Restart(SystemService<'a>),
    Enable(SystemService<'a>),
    Disable(SystemService<'a>),
    IsActive(SystemService<'a>),
}

impl ServiceCommand<'_> {
    fn try_exec_command(
        self,
        service_manager: &GeneralServiceManager,
    ) -> Result<ExecCommand, SystemServiceError> {
        let config_path = service_manager.config_path.clone();
        let config = match self {
            Self::CheckManager => &service_manager.init_config.is_available,
            Self::Stop(_) => &service_manager.init_config.stop,
            Self::Restart(_) => &service_manager.init_config.restart,
            Self::Start(_) => &service_manager.init_config.start,
            Self::Enable(_) => &service_manager.init_config.enable,
            Self::Disable(_) => &service_manager.init_config.disable,
            Self::IsActive(_) => &service_manager.init_config.is_active,
        };

        match self {
            Self::CheckManager => ExecCommand::try_new(config.clone(), self, config_path),
            Self::Stop(service)
            | Self::Restart(service)
            | Self::Start(service)
            | Self::Enable(service)
            | Self::Disable(service)
            | Self::IsActive(service) => {
                ExecCommand::try_new_with_placeholder(config.clone(), self, config_path, service)
            }
        }
    }
}

impl fmt::Display for ServiceCommand<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::CheckManager => write!(f, "is_available"),
            Self::Stop(_service) => write!(f, "stop"),
            Self::Start(_service) => write!(f, "start"),
            Self::Restart(_service) => write!(f, "restart"),
            Self::Enable(_service) => write!(f, "enable"),
            Self::Disable(_service) => write!(f, "disable"),
            Self::IsActive(_service) => write!(f, "is_active"),
        }
    }
}

impl GeneralServiceManager {
    async fn run_service_command_as_root(
        &self,
        exec_command: ExecCommand,
        config_path: &str,
    ) -> Result<ServiceCommandExitStatus, SystemServiceError> {
        match exec_command.to_command().status().await {
            Ok(status) => Ok(ServiceCommandExitStatus {
                status,
                service_command: exec_command.to_string(),
            }),
            Err(_) => Err(SystemServiceError::ServiceCommandNotFound {
                service_command: exec_command.to_string(),
                path: config_path.to_string(),
            }),
        }
    }
}

#[derive(Debug)]
struct ServiceCommandExitStatus {
    status: ExitStatus,
    service_command: String,
}

impl ServiceCommandExitStatus {
    fn must_succeed(self) -> Result<(), SystemServiceError> {
        if self.status.success() {
            Ok(())
        } else {
            match self.status.code() {
                Some(code) => Err(SystemServiceError::ServiceCommandFailedWithCode {
                    service_command: self.service_command,
                    code,
                }),
                None => Err(SystemServiceError::ServiceCommandFailedBySignal {
                    service_command: self.service_command,
                }),
            }
        }
    }

    fn success(self) -> bool {
        self.status.success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::*;
    use test_case::test_case;

    #[test_case(
    vec!["bin".to_string(), "{}".to_string(), "arg2".to_string()],
    vec!["bin".to_string(), "mosquitto".to_string(), "arg2".to_string()]
    ;"one placeholder")]
    #[test_case(
    vec!["bin".to_string(), "{}".to_string(), "{}".to_string()],
    vec!["bin".to_string(), "mosquitto".to_string(), "mosquitto".to_string()]
    ;"several placeholders")]
    fn replace_placeholder_with_service(input: Vec<String>, expected_output: Vec<String>) {
        let replaced_config = replace_with_service_name(
            &input,
            ServiceCommand::Stop(SystemService::Mosquitto),
            "/dummy/path.toml",
            SystemService::Mosquitto,
        )
        .unwrap();
        assert_eq!(replaced_config, expected_output)
    }

    #[test]
    fn fail_to_replace_placeholder_with_service() {
        let input = vec!["bin".to_string(), "arg1".to_string(), "arg2".to_string()];
        let system_config_error = replace_with_service_name(
            &input,
            ServiceCommand::Stop(SystemService::Mosquitto),
            "dummy/path.toml",
            SystemService::Mosquitto,
        )
        .unwrap_err();
        assert_matches!(
            system_config_error,
            SystemServiceError::SystemConfigInvalidSyntax { .. }
        )
    }

    #[test_case(
    vec!["bin".to_string(), "arg1".to_string(), "arg2".to_string()],
    ExecCommand {
        exec: "bin".to_string(),
        args: vec!["arg1".to_string(), "arg2".to_string()]
    }
    ;"with arguments")]
    #[test_case(
    vec!["bin".to_string()],
    ExecCommand {
        exec: "bin".to_string(),
        args: vec![]
    }
    ;"only executable")]
    fn build_exec_command(config: Vec<String>, expected: ExecCommand) {
        let exec_command = ExecCommand::try_new(
            config,
            ServiceCommand::Stop(SystemService::Mosquitto),
            "test/dummy.toml".into(),
        )
        .unwrap();
        assert_eq!(exec_command, expected);
    }

    #[test]
    fn fail_to_build_exec_command() {
        let config = vec![];
        let system_config_error = ExecCommand::try_new(
            config,
            ServiceCommand::Stop(SystemService::Mosquitto),
            "test/dummy.toml".into(),
        )
        .unwrap_err();
        assert_matches!(
            system_config_error,
            SystemServiceError::SystemConfigInvalidSyntax { .. }
        );
    }

    #[test_case(
    ExecCommand {
        exec: "bin".to_string(),
        args: vec!["arg1".to_string(), "arg2".to_string()]
    },
    "bin arg1 arg2"
    ;"with arguments")]
    #[test_case(
    ExecCommand {
        exec: "bin".to_string(),
        args: vec![]
    },
    "bin"
    ;"only executable")]
    fn print_exec_command(exec_command: ExecCommand, expected: &str) {
        assert_eq!(exec_command.to_string(), expected)
    }
}
