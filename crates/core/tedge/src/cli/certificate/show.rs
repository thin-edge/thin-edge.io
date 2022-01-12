use super::error::CertError;
use crate::command::Command;

use certificate::PemCertificate;
use tedge_config::*;

/// Show the device certificate, if any
pub struct ShowCertCmd {
    /// The path where the device certificate will be stored
    pub cert_path: FilePath,
}

impl Command for ShowCertCmd {
    fn description(&self) -> String {
        "show the device certificate".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        let () = self.show_certificate()?;
        Ok(())
    }
}

impl ShowCertCmd {
    fn show_certificate(&self) -> Result<(), CertError> {
        let pem = PemCertificate::from_pem_file(&self.cert_path).map_err(|err| match err {
            certificate::CertificateError::IoError(from) => {
                CertError::IoError(from).cert_context(self.cert_path.clone())
            }
            from => CertError::CertificateError(from),
        })?;

        println!("Device certificate: {}", self.cert_path);
        println!("Subject: {}", pem.subject()?);
        println!("Issuer: {}", pem.issuer()?);
        println!("Valid from: {}", pem.not_before()?);
        println!("Valid up to: {}", pem.not_after()?);
        println!("Thumbprint: {}", pem.thumbprint()?);
        Ok(())
    }
}
