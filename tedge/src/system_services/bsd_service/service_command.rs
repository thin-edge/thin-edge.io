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
            Self::Stop(service) => format!("{} {} stop", SERVICE_BIN, service_name_for(service)),
            Self::Restart(service) => {
                format!("{} {} restart", SERVICE_BIN, service_name_for(service))
            }
            Self::Enable(service) => {
                format!("{} {} enable", SERVICE_BIN, service_name_for(service))
            }
            Self::Disable(service) => {
                format!("{} {} forcedisable", SERVICE_BIN, service_name_for(service))
            }
            Self::IsActive(service) => {
                format!("{} {} status", SERVICE_BIN, service_name_for(service))
            }
        }
    }

    pub fn into_command(self) -> std::process::Command {
        match self {
            Self::CheckManager => CommandBuilder::new(SERVICE_BIN).arg("-l").silent().build(),
            Self::Stop(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service_name_for(service))
                .arg("stop")
                .silent()
                .build(),
            Self::Restart(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service_name_for(service))
                .arg("restart")
                .silent()
                .build(),
            Self::Enable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service_name_for(service))
                .arg("enable")
                .silent()
                .build(),

            Self::Disable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service_name_for(service))
                //
                // Use "forcedisable" as otherwise it could fail if you have a commented out
                // `# mosquitto_enable="YES"` or
                // `# mosquitto_enable="NO"` in your `/etc/rc.conf` file.
                //
                .arg("forcedisable")
                .silent()
                .build(),
            Self::IsActive(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(service_name_for(service))
                .arg("status")
                .silent()
                .build(),
        }
    }
}

fn service_name_for(service: SystemService) -> &'static str {
    match service {
        SystemService::Mosquitto => "mosquitto",
        SystemService::TEdgeMapper => "tedge-mapper",
    }
}
