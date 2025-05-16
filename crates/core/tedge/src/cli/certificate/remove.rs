use std::io::ErrorKind::NotFound;
use tedge_config::TEdgeConfig;
use tokio::fs;

use super::error::CertError;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::CertificateShift;
use camino::Utf8PathBuf;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: Utf8PathBuf,

    /// The path of the private key to be removed
    pub key_path: Utf8PathBuf,
}

#[async_trait::async_trait]
impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.remove_certificate().await? {
            RemoveCertResult::Removed => eprintln!("Certificate was successfully removed"),
            RemoveCertResult::NotFound => eprintln!("There is no certificate to remove"),
        }
        Ok(())
    }
}

impl RemoveCertCmd {
    pub(crate) async fn remove_certificate(&self) -> Result<RemoveCertResult, CertError> {
        let _new_cert_silently_removed =
            fs::remove_file(&CertificateShift::new_certificate_path(&self.cert_path)).await;
        let cert_removed = fs::remove_file(&self.cert_path).await;
        let key_removed = fs::remove_file(&self.key_path).await;
        match cert_removed.and(key_removed) {
            Ok(()) => Ok(RemoveCertResult::Removed),
            Err(err) if err.kind() == NotFound => Ok(RemoveCertResult::NotFound),
            Err(err) => Err(err.into()),
        }
    }
}

pub(crate) enum RemoveCertResult {
    Removed,
    NotFound,
}
