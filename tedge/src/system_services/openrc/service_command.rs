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
        match self {
            Self::CheckManager => format!("{} -l", RC_SERVICE_BIN),
            Self::Stop(service) => format!("{} {} stop", RC_SERVICE_BIN, service.as_service_name()),
            Self::Restart(service) => {
                format!("{} {} restart", RC_SERVICE_BIN, service.as_service_name())
            }
            Self::Enable(service) => format!("{} add {}", RC_UPDATE_BIN, service.as_service_name()),
            Self::Disable(service) => {
                format!("{} delete {}", RC_UPDATE_BIN, service.as_service_name())
            }
            Self::IsActive(service) => {
                format!("{} {} status", RC_SERVICE_BIN, service.as_service_name())
            }
        }
    }

    pub fn into_command(self) -> std::process::Command {
        match self {
            Self::CheckManager => CommandBuilder::new(RC_SERVICE_BIN)
                .arg("-l")
                .silent()
                .build(),
            Self::Stop(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("stop")
                .silent()
                .build(),
            Self::Restart(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("restart")
                .silent()
                .build(),
            Self::Enable(service) => CommandBuilder::new(RC_UPDATE_BIN)
                .arg("add")
                .arg(service.as_service_name())
                .silent()
                .build(),
            Self::Disable(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg("delete")
                .arg(service.as_service_name())
                .silent()
                .build(),
            Self::IsActive(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("status")
                .silent()
                .build(),
        }
    }
}
