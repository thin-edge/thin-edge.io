use std::io::ErrorKind::NotFound;
use tokio::fs;

use crate::command::CommandAsync;
use crate::log::MaybeFancy;
use camino::Utf8PathBuf;

use super::error::CertError;

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    pub cert_path: Utf8PathBuf,

    /// The path of the private key to be removed
    pub key_path: Utf8PathBuf,
}

#[async_trait::async_trait]
impl CommandAsync for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.remove_certificate().await? {
            RemoveCertResult::Removed => eprintln!("Certificate was successfully removed"),
            RemoveCertResult::NotFound => eprintln!("There is no certificate to remove"),
        }
        Ok(())
    }
}

impl RemoveCertCmd {
    pub(crate) async fn remove_certificate(&self) -> Result<RemoveCertResult, CertError> {
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
