use crate::system_services::*;
use std::io::Write;

/// Extension trait for `SystemServiceManager`.
pub trait SystemServiceManagerExt {
    fn start_and_enable_service<W: Write>(&self, service: SystemService, wr: W) -> bool;
    fn stop_and_disable_service<W: Write>(&self, service: SystemService, wr: W) -> bool;
}

impl SystemServiceManagerExt for &dyn SystemServiceManager {
    fn start_and_enable_service<W: Write>(&self, service: SystemService, mut wr: W) -> bool {
        let mut failed = false;

        let _ = writeln!(&mut wr, "Starting {} service.\n", service);
        if let Err(err) = self.restart_service(service) {
            let _ = writeln!(&mut wr, "Failed to stop {} service: {:?}", service, err);
            failed = true;
        }

        let _ = writeln!(&mut wr, "Persisting {} on reboot.\n", service);
        if let Err(err) = self.enable_service(service) {
            let _ = writeln!(&mut wr, "Failed to enable {} service: {:?}", service, err);
            failed = true;
        }

        if !failed {
            let _ = writeln!(
                &mut wr,
                "{} service successfully started and enabled!\n",
                service
            );
        }

        failed
    }

    fn stop_and_disable_service<W: Write>(&self, service: SystemService, mut wr: W) -> bool {
        let mut failed = false;

        let _ = writeln!(&mut wr, "Stopping {} service.\n", service);
        if let Err(err) = self.stop_service(service) {
            let _ = writeln!(&mut wr, "Failed to stop {} service: {:?}", service, err);
            failed = true;
        }

        let _ = writeln!(&mut wr, "Disabling {} service.\n", service);
        if let Err(err) = self.disable_service(service) {
            let _ = writeln!(&mut wr, "Failed to disable {} service: {:?}", service, err);
            failed = true;
        }

        if !failed {
            let _ = writeln!(
                &mut wr,
                "{} service successfully stopped and disabled!\n",
                service
            );
        }

        failed
    }
}
