use super::error::CertError;
use crate::command::Command;
use tedge_config::*;
use tedge_utils::paths::ok_if_not_found;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: FilePath,

    /// The path of the private key to be removed
    pub key_path: FilePath,

    /// The path of the certificate signing request to be removed
    pub csr_path: FilePath,
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.remove_certificate()?;
        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self) -> Result<(), CertError> {
        std::fs::remove_file(&self.cert_path).or_else(ok_if_not_found)?;
        std::fs::remove_file(&self.key_path).or_else(ok_if_not_found)?;
        std::fs::remove_file(&self.csr_path).or_else(ok_if_not_found)?;

        Ok(())
    }
}
