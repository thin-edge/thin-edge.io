use crate::system_services::{
    CommandBuilder, SystemService, SystemServiceError, SystemServiceManager,
};

use super::config::{InitConfig, SystemConfig, SERVICE_CONFIG_FILE};
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;

#[derive(Debug)]
pub struct GeneralServiceManager {
    init_config: InitConfig,
    config_path: String,
}

impl GeneralServiceManager {
    pub fn try_new(config_root: PathBuf) -> Result<Self, SystemServiceError> {
        let init_config = SystemConfig::try_new(config_root.clone())?.init;

        let config_path = config_root
            .join(SERVICE_CONFIG_FILE)
            .to_str()
            .unwrap_or(SERVICE_CONFIG_FILE)
            .to_string();

        Ok(Self {
            init_config,
            config_path,
        })
    }
}

impl SystemServiceManager for GeneralServiceManager {
    fn name(&self) -> &str {
        &self.init_config.name
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::CheckManager.try_exec_command(self)?;

        match exec_command.to_command().status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(SystemServiceError::ServiceManagerUnavailable {
                cmd: exec_command.to_string(),
                name: self.name().to_string(),
            }),
        }
    }

    fn stop_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Stop(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())?
            .must_succeed()
    }

    fn restart_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Restart(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())?
            .must_succeed()
    }

    fn enable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Enable(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())?
            .must_succeed()
    }

    fn disable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let exec_command = ServiceCommand::Disable(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())?
            .must_succeed()
    }

    fn is_service_running(&self, service: SystemService) -> Result<bool, SystemServiceError> {
        let exec_command = ServiceCommand::IsActive(service).try_exec_command(self)?;
        self.run_service_command_as_root(exec_command, self.config_path.as_str())
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
        config_path: String,
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

    fn try_new_with_placeholder(
        config: Vec<String>,
        service_cmd: ServiceCommand,
        config_path: String,
        service: SystemService,
    ) -> Result<Self, SystemServiceError> {
        let replaced =
            replace_with_service_name(&config, service_cmd, config_path.as_str(), service)?;
        Self::try_new(replaced, service_cmd, config_path)
    }

    fn to_command(&self) -> std::process::Command {
        CommandBuilder::new(&self.exec)
            .args(&self.args)
            .silent()
            .build()
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

fn replace_with_service_name(
    input_args: &[String],
    service_cmd: ServiceCommand,
    config_path: &str,
    service: SystemService,
) -> Result<Vec<String>, SystemServiceError> {
    if !input_args.iter().any(|s| s == "{}") {
        return Err(SystemServiceError::SystemConfigInvalidSyntax {
            reason: "A placeholder '{}' is missing.".to_string(),
            cmd: service_cmd.to_string(),
            path: config_path.to_string(),
        });
    }

    let mut args = input_args.to_owned();
    for item in args.iter_mut() {
        if item == "{}" {
            *item = SystemService::as_service_name(service).to_string();
        }
    }

    Ok(args)
}

#[derive(Debug, Copy, Clone)]
enum ServiceCommand {
    CheckManager,
    Stop(SystemService),
    Restart(SystemService),
    Enable(SystemService),
    Disable(SystemService),
    IsActive(SystemService),
}

impl ServiceCommand {
    fn try_exec_command(
        &self,
        service_manager: &GeneralServiceManager,
    ) -> Result<ExecCommand, SystemServiceError> {
        let config_path = service_manager.config_path.clone();
        match self {
            Self::CheckManager => ExecCommand::try_new(
                service_manager.init_config.is_available.clone(),
                ServiceCommand::CheckManager,
                config_path,
            ),
            Self::Stop(service) => ExecCommand::try_new_with_placeholder(
                service_manager.init_config.stop.clone(),
                ServiceCommand::Stop(*service),
                config_path,
                *service,
            ),
            Self::Restart(service) => ExecCommand::try_new_with_placeholder(
                service_manager.init_config.restart.clone(),
                ServiceCommand::Restart(*service),
                config_path,
                *service,
            ),
            Self::Enable(service) => ExecCommand::try_new_with_placeholder(
                service_manager.init_config.enable.clone(),
                ServiceCommand::Enable(*service),
                config_path,
                *service,
            ),
            Self::Disable(service) => ExecCommand::try_new_with_placeholder(
                service_manager.init_config.disable.clone(),
                ServiceCommand::Disable(*service),
                config_path,
                *service,
            ),
            Self::IsActive(service) => ExecCommand::try_new_with_placeholder(
                service_manager.init_config.is_active.clone(),
                ServiceCommand::IsActive(*service),
                config_path,
                *service,
            ),
        }
    }
}

impl fmt::Display for ServiceCommand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::CheckManager => write!(f, "is_available"),
            Self::Stop(_service) => write!(f, "stop"),
            Self::Restart(_service) => write!(f, "restart"),
            Self::Enable(_service) => write!(f, "enable"),
            Self::Disable(_service) => write!(f, "disable"),
            Self::IsActive(_service) => write!(f, "is_active"),
        }
    }
}

impl GeneralServiceManager {
    fn run_service_command_as_root(
        &self,
        exec_command: ExecCommand,
        config_path: &str,
    ) -> Result<ServiceCommandExitStatus, SystemServiceError> {
        exec_command
            .to_command()
            .status()
            .map_err(|_| SystemServiceError::ServiceCommandNotFound {
                service_command: exec_command.to_string(),
                path: config_path.to_string(),
            })
            .map(|status| ServiceCommandExitStatus {
                status,
                service_command: exec_command.to_string(),
            })
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
            "test/dummy.toml".to_string(),
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
            "test/dummy.toml".to_string(),
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
    r#""bin" "arg1" "arg2""#
    ;"with arguments")]
    #[test_case(
    ExecCommand {
        exec: "bin".to_string(),
        args: vec![]
    },
    r#""bin""#
    ;"only executable")]
    fn construct_command(exec_command: ExecCommand, expected: &str) {
        let command = exec_command.to_command();
        assert_eq!(format!("{:?}", command), expected);
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
