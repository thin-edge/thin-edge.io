use super::error::CertError;
use crate::command::Command;
use crate::log::MaybeFancy;

use camino::Utf8PathBuf;
use certificate::PemCertificate;
use certificate::ValidityStatus;

/// Show the device certificate, if any
pub struct ShowCertCmd {
    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,
}

impl Command for ShowCertCmd {
    fn description(&self) -> String {
        "show the device certificate".into()
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.show_certificate()?;
        Ok(())
    }
}

impl ShowCertCmd {
    fn show_certificate(&self) -> Result<(), CertError> {
        let pem = PemCertificate::from_pem_file(&self.cert_path).map_err(|err| match err {
            certificate::CertificateError::IoError { error, .. } => {
                CertError::IoError(error).cert_context(self.cert_path.clone())
            }
            from => CertError::CertificateError(from),
        })?;

        println!("Device certificate: {}", self.cert_path);
        println!("Subject: {}", pem.subject()?);
        println!("Issuer: {}", pem.issuer()?);
        println!("Status: {}", display_status(pem.still_valid()?));
        println!("Valid from: {}", pem.not_before()?);
        println!("Valid up to: {}", pem.not_after()?);
        println!("Serial number: {} (0x{})", pem.serial()?, pem.serial_hex()?);
        println!("Thumbprint: {}", pem.thumbprint()?);
        Ok(())
    }
}

fn display_status(status: ValidityStatus) -> String {
    match status {
        ValidityStatus::Valid { expired_in } => {
            format!(
                "VALID (expires in: {})",
                humantime::format_duration(expired_in)
            )
        }
        ValidityStatus::Expired { since } => {
            format!("EXPIRED (since: {})", humantime::format_duration(since))
        }
        ValidityStatus::NotValidYet { valid_in } => {
            format!(
                "NOT VALID YET (will be in: {})",
                humantime::format_duration(valid_in)
            )
        }
    }
}
