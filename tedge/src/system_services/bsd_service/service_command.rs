use crate::system_command::*;
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
        SystemCommand::from(self).command_line().join(" ")
    }
}

impl From<ServiceCommand> for SystemCommand {
    fn from(service_command: ServiceCommand) -> SystemCommand {
        match service_command {
            ServiceCommand::CheckManager => SystemCommand::new(SERVICE_BIN).arg("-l"),
            ServiceCommand::Stop(service) => SystemCommand::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("stop"),
            ServiceCommand::Restart(service) => SystemCommand::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("restart"),
            ServiceCommand::Enable(service) => SystemCommand::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("enable"),
            ServiceCommand::Disable(service) => SystemCommand::new(SERVICE_BIN)
                .arg(service.as_service_name())
                //
                // Use "forcedisable" as otherwise it could fail if you have a commented out
                // `# mosquitto_enable="YES"` or
                // `# mosquitto_enable="NO"` in your `/etc/rc.conf` file.
                //
                .arg("forcedisable"),
            ServiceCommand::IsActive(service) => SystemCommand::new(SERVICE_BIN)
                .arg(service.as_service_name())
                .arg("status"),
        }
        .role(Role::Root)
    }
}
