use crate::system_command::*;
use crate::system_services::*;

const RC_SERVICE_BIN: &str = "/sbin/rc-service";
const RC_UPDATE_BIN: &str = "/sbin/rc-update";

#[derive(Debug, Copy, Clone)]
pub enum ServiceCommand {
    CheckManager,
    Stop(SystemService),
    Restart(SystemService),
    Enable(SystemService),
    Disable(SystemService),
    IsActive(SystemService),
}

impl ServiceCommand {
    pub fn to_string(self) -> String {
        SystemCommand::from(self).command_line().join(" ")
    }
}

impl From<ServiceCommand> for SystemCommand {
    fn from(service_command: ServiceCommand) -> SystemCommand {
        match service_command {
            ServiceCommand::CheckManager => SystemCommand::new(RC_SERVICE_BIN).arg("-l"),
            ServiceCommand::Stop(service) => SystemCommand::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("stop"),
            ServiceCommand::Restart(service) => SystemCommand::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("restart"),
            ServiceCommand::Enable(service) => SystemCommand::new(RC_UPDATE_BIN)
                .arg("add")
                .arg(service.as_service_name()),
            ServiceCommand::Disable(service) => SystemCommand::new(RC_SERVICE_BIN)
                .arg("delete")
                .arg(service.as_service_name()),
            ServiceCommand::IsActive(service) => SystemCommand::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("status"),
        }
        .role(Role::Root)
    }
}
