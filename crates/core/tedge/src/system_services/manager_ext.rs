use crate::system_services::*;
use anyhow::Context as _;

/// Extension trait for `SystemServiceManager`.
pub trait SystemServiceManagerExt {
    fn start_and_enable_service(&self, service: SystemService<'_>) -> anyhow::Result<()>;
    fn stop_and_disable_service(&self, service: SystemService<'_>) -> anyhow::Result<()>;
}

impl SystemServiceManagerExt for &dyn SystemServiceManager {
    fn start_and_enable_service(&self, service: SystemService<'_>) -> anyhow::Result<()> {
        self.start_service(service)
            .with_context(|| format!("Failed to start {service}"))?;
        self.enable_service(service)
            .with_context(|| format!("Failed to enable {service}"))
    }

    fn stop_and_disable_service(&self, service: SystemService<'_>) -> anyhow::Result<()> {
        self.stop_service(service)
            .with_context(|| format!("Failed to stop {service}"))?;
        self.disable_service(service)
            .with_context(|| format!("Failed to disable {service}"))
    }
}
