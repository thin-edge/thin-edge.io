use crate::system_services::{
    CommandBuilder, SystemConfig, SystemConfigError, SystemService, SystemServiceError,
    SystemServiceManager,
};
use itertools::Itertools;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;
use tedge_users::{UserManager, ROOT_USER};

#[derive(Debug)]
pub struct GeneralServiceManager {
    user_manager: UserManager,
    system_config: SystemConfig,
}

impl GeneralServiceManager {
    pub fn try_new(
        user_manager: UserManager,
        config_root: PathBuf,
    ) -> Result<Self, SystemConfigError> {
        let system_config = SystemConfig::new(config_root);
        Ok(Self {
            user_manager,
            system_config,
        })
    }
}

impl SystemServiceManager for GeneralServiceManager {
    fn name(&self) -> &str {
        &self.system_config.name
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        let exec_command = ExecCommand::try_new(self.system_config.is_available.clone())?;

        match exec_command.to_command().status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(SystemServiceError::ServiceManagerUnavailable(
                self.name().to_string(),
            )),
        }
    }

    fn stop_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let config = replace_with_service_name(&self.system_config.stop, service)?;
        let exec_command = ExecCommand::try_new(config)?;
        self.run_service_command_as_root(exec_command)?
            .must_succeed()
    }

    fn restart_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let config = replace_with_service_name(&self.system_config.restart, service)?;
        let exec_command = ExecCommand::try_new(config)?;
        self.run_service_command_as_root(exec_command)?
            .must_succeed()
    }

    fn enable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let config = replace_with_service_name(&self.system_config.enable, service)?;
        let exec_command = ExecCommand::try_new(config)?;
        self.run_service_command_as_root(exec_command)?
            .must_succeed()
    }

    fn disable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let config = replace_with_service_name(&self.system_config.disable, service)?;
        let exec_command = ExecCommand::try_new(config)?;
        self.run_service_command_as_root(exec_command)?
            .must_succeed()
    }

    fn is_service_running(&self, service: SystemService) -> Result<bool, SystemServiceError> {
        let config = replace_with_service_name(&self.system_config.is_active, service)?;
        let exec_command = ExecCommand::try_new(config)?;
        self.run_service_command_as_root(exec_command)
            .map(|status| status.success())
    }
}

#[derive(Debug, PartialEq)]
struct ExecCommand {
    exec: String,
    args: Vec<String>,
}

impl ExecCommand {
    fn try_new(config: Vec<String>) -> Result<Self, SystemConfigError> {
        match config.split_first() {
            Some((exec, args)) => Ok(Self {
                exec: exec.to_string(),
                args: args.to_vec(),
            }),
            None => Err(SystemConfigError::InvalidSyntax {
                reason: "Requires 1 or more arguments.".to_string(),
            }),
        }
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
            write!(f, "{} {}", self.exec, self.args.iter().format(" "))
        }
    }
}

fn replace_with_service_name(
    input_args: &[String],
    service: SystemService,
) -> Result<Vec<String>, SystemConfigError> {
    if !input_args.iter().any(|s| s == "{}") {
        return Err(SystemConfigError::InvalidSyntax {
            reason: "A placeholder '{}' is missing.".to_string(),
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

impl GeneralServiceManager {
    fn run_service_command_as_root(
        &self,
        exec_command: ExecCommand,
    ) -> Result<ServiceCommandExitStatus, SystemServiceError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);

        exec_command
            .to_command()
            .status()
            .map_err(Into::into)
            .map(|status| ServiceCommandExitStatus {
                status,
                service_command: exec_command.to_string(),
            })
    }
}

struct ServiceCommandExitStatus {
    status: ExitStatus,
    service_command: String,
}

impl ServiceCommandExitStatus {
    fn must_succeed(self) -> Result<(), SystemServiceError> {
        if self.status.success() {
            Ok(())
        } else {
            Err(SystemServiceError::ServiceCommandFailed {
                service_command: self.service_command,
                code: self.status.code(),
            })
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
        let replaced_config = replace_with_service_name(&input, SystemService::Mosquitto).unwrap();
        assert_eq!(replaced_config, expected_output)
    }

    #[test]
    fn fail_to_replace_placeholder_with_service() {
        let input = vec!["bin".to_string(), "arg1".to_string(), "arg2".to_string()];
        let system_config_error =
            replace_with_service_name(&input, SystemService::Mosquitto).unwrap_err();
        assert_matches!(system_config_error, SystemConfigError::InvalidSyntax { .. })
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
        let exec_command = ExecCommand::try_new(config).unwrap();
        assert_eq!(exec_command, expected);
    }

    #[test]
    fn fail_to_build_exec_command() {
        let config = vec![];
        let system_config_error = ExecCommand::try_new(config).unwrap_err();
        assert_matches!(system_config_error, SystemConfigError::InvalidSyntax { .. });
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
