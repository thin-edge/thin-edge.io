use super::file_installer::*;
use crate::command::{Command, ExecutionContext};

use tedge_config::*;

use super::error::CertError;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: FilePath,

    /// The path of the private key to be removed
    pub key_path: FilePath,
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let () = self.remove_certificate(&Installer)?;
        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self, installer: &dyn FileInstaller) -> Result<(), CertError> {
        let () = installer.remove_if_exists(self.cert_path.as_ref())?;
        let () = installer.remove_if_exists(self.key_path.as_ref())?;
        Ok(())
    }
}
