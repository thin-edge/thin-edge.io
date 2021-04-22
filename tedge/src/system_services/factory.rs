use crate::system_services::*;
use crate::utils::users::UserManager;

pub trait SystemServiceManagerFactory {
    fn create(&self) -> Box<dyn SystemServiceManager>;
}

pub struct DefaultSystemServiceManagerFactory {
    user_manager: UserManager,
}

impl DefaultSystemServiceManagerFactory {
    pub fn new(user_manager: UserManager) -> Self {
        Self { user_manager }
    }
}

#[cfg(all(feature = "systemd", feature = "openrc"))]
compile_error!("Both features \"systemd\" and \"openrc\" cannot be enabled at the same time.");

impl SystemServiceManagerFactory for DefaultSystemServiceManagerFactory {
    fn create(&self) -> Box<dyn SystemServiceManager> {
        if cfg!(feature = "systemd") {
            Box::new(SystemdManager::new(self.user_manager.clone()))
        } else if cfg!(feature = "openrc") {
            Box::new(OpenRcServiceManager::new(self.user_manager.clone()))
        } else {
            panic!("Neither feature \"systemd\" nor \"openrc\" are enabled.");
        }
    }
}
