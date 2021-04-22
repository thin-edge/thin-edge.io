use crate::system_services::*;

const SERVICE_BIN: &str = "/usr/sbin/service";

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
            Self::CheckManager => format!("{} -l", SERVICE_BIN),
            Self::Stop(service) => format!("{} {} stop", SERVICE_BIN, service.as_service_name()),
            Self::Restart(service) => {
                format!("{} {} restart", SERVICE_BIN, service.as_service_name())
            }
            Self::Enable(service) => {
                format!("{} {} enable", SERVICE_BIN, service.as_service_name())
            }
            Self::Disable(service) => {
                format!("{} {} forcedisable", SERVICE_BIN, service.as_service_name())
            }
            Self::IsActive(service) => {
                format!("{} {} status", SERVICE_BIN, service.as_service_name())
            }
        }
    }

    pub fn into_command(self) -> std::process::Command {
        match self {
            Self::CheckManager => CommandBuilder::new(SERVICE_BIN).arg("-l").silent().build(),
            Self::Stop(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("stop")
                .silent()
                .build(),
            Self::Restart(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("restart")
                .silent()
                .build(),
            Self::Enable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("enable")
                .silent()
                .build(),

            Self::Disable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service.as_service_name())
                //
                // Use "forcedisable" as otherwise it could fail if you have a commented out
                // `# mosquitto_enable="YES"` or
                // `# mosquitto_enable="NO"` in your `/etc/rc.conf` file.
                //
                .arg("forcedisable")
                .silent()
                .build(),
            Self::IsActive(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("status")
                .silent()
                .build(),
        }
    }
}
