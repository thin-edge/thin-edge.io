use crate::system_command::*;
use crate::system_services::*;
use service_command::*;
use std::process::*;
use std::sync::Arc;

mod service_command;

/// Service manager that uses `service(8)` as found on FreeBSD to control system services.
///
pub struct BsdServiceManager {
    system_command_runner: Arc<dyn SystemCommandRunner>,
}

impl BsdServiceManager {
    pub fn new(system_command_runner: Arc<dyn SystemCommandRunner>) -> Self {
        Self {
            system_command_runner,
        }
    }
}

impl SystemServiceManager for BsdServiceManager {
    fn manager_name(&self) -> &str {
        "service(8)"
    }

    fn check_manager_available(&mut self) -> Result<(), ServicesError> {
        let system_command = SystemCommand::from(ServiceCommand::CheckManager);

        match self.system_command_runner.run(system_command) {
            Ok(status) if status.success() => Ok(()),
            _ => Err(BsdServiceError::ServiceManagerNotAvailable.into()),
        }
    }

    fn stop_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Stop(service);

        self.run_service_command(service_command)?.must_succeed()
    }

    fn restart_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Restart(service);

        self.run_service_command(service_command)?.must_succeed()
    }

    fn enable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Enable(service);

        self.run_service_command(service_command)?.must_succeed()
    }

    fn disable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Disable(service);

        self.run_service_command(service_command)?.must_succeed()
    }

    fn is_service_active(&mut self, service: SystemService) -> Result<bool, ServicesError> {
        let service_command = ServiceCommand::IsActive(service);

        self.run_service_command(service_command)
            .map(|status| status.success())
    }
}

impl BsdServiceManager {
    fn run_service_command(
        &self,
        service_command: ServiceCommand,
    ) -> Result<ServiceCommandExitStatus, ServicesError> {
        self.system_command_runner
            .run(service_command.into())
            .map_err(Into::into)
            .map(|status| ServiceCommandExitStatus {
                status,
                service_command,
            })
    }
}

struct ServiceCommandExitStatus {
    status: ExitStatus,
    service_command: ServiceCommand,
}

impl ServiceCommandExitStatus {
    fn must_succeed(self) -> Result<(), ServicesError> {
        if self.status.success() {
            Ok(())
        } else {
            Err(BsdServiceError::ServiceCommandFailed {
                service_command: self.service_command.to_string(),
                code: self.status.code(),
            }
            .into())
        }
    }

    fn success(self) -> bool {
        self.status.success()
    }
}
