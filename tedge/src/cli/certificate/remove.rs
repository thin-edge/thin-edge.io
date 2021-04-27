use super::cert_store::*;
use crate::command::{Command, ExecutionContext};

use tedge_config::*;

use super::error::CertError;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: FilePath,

    /// The path of the private key to be removed
    pub key_path: FilePath,

    /// The certificate store of the mosquitto broker
    pub broker_cert_store: Box<dyn CertificateStore>,
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let () = self.remove_certificate()?;
        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self) -> Result<(), CertError> {
        let () = self
            .broker_cert_store
            .remove_certificate(self.cert_path.as_ref())?;
        let () = self
            .broker_cert_store
            .remove_private_key(self.key_path.as_ref())?;
        Ok(())
    }
}
