use crate::system_command::*;
use crate::system_services::{bsd_service::*, openrc::*, systemd::*, *};
use std::sync::Arc;

pub trait SystemServiceManagerFactory {
    fn create(&self) -> Box<dyn SystemServiceManager>;
}

pub struct DefaultSystemServiceManagerFactory {
    system_command_runner: Arc<dyn SystemCommandRunner>,
}

impl DefaultSystemServiceManagerFactory {
    pub fn new(system_command_runner: Arc<dyn SystemCommandRunner>) -> Self {
        Self {
            system_command_runner,
        }
    }
}

#[cfg(all(feature = "systemd", feature = "openrc"))]
compile_error!("Both features \"systemd\" and \"openrc\" cannot be enabled at the same time.");

#[cfg(not(any(feature = "systemd", feature = "openrc", target_os = "freebsd")))]
compile_error!("Unsupported system.");

impl SystemServiceManagerFactory for DefaultSystemServiceManagerFactory {
    fn create(&self) -> Box<dyn SystemServiceManager> {
        if cfg!(feature = "systemd") {
            Box::new(SystemdManager::new(self.system_command_runner.clone()))
        } else if cfg!(feature = "openrc") {
            Box::new(OpenRcServiceManager::new(
                self.system_command_runner.clone(),
            ))
        } else if cfg!(target_os = "freebsd") {
            Box::new(BsdServiceManager::new(self.system_command_runner.clone()))
        } else {
            panic!("Neither feature \"systemd\" nor \"openrc\" are enabled.");
        }
    }
}
