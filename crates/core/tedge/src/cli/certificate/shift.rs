//! Thin-edge maintains 2 certificates (*current* and *new*) for each cloud connection.
//!
//! The paths to these certificates are derived from the `device.cert_path` tedge config setting.
//!   - `"$(tedge config get device.cert_path)"` is the path to the certificate currently used to connect the cloud endpoint
//!   - `"$(tedge config get device.cert_path).new"` is the path to a new certificate, if any, still to be validated.
//!
//! The command `tedge cert renew` stores the new certificate into `"$(tedge config get device.cert_path).new"`
//!
//! The promotion of a new certificate as the current certificate is done by the `tedge connect` command.
//! If there is a new certificate, `tedge connect` uses this new certificate to connect the cloud
//!  and promotes it as the current on a successful connection.

use camino::Utf8Path;
use camino::Utf8PathBuf;

/// Holds the paths to a pair of certificates,
/// the first one being active i.e. still used to connect the cloud
/// and a second one which has to be validated before being used.
pub struct CertificateShift {
    pub active_cert_path: Utf8PathBuf,
    pub new_cert_path: Utf8PathBuf,
}

impl CertificateShift {
    pub async fn exists_new_certificate(cert_path: &Utf8Path) -> Option<CertificateShift> {
        let active_cert_path = cert_path.to_owned();
        let mut new_cert_path = active_cert_path.clone();
        new_cert_path.set_file_name(match active_cert_path.as_path().file_name() {
            None => "certificate.new".to_string(),
            Some(filename) => format!("{filename}.new"),
        });

        if let Ok(true) = tokio::fs::try_exists(new_cert_path.as_path()).await {
            Some(CertificateShift {
                active_cert_path,
                new_cert_path,
            })
        } else {
            None
        }
    }

    pub async fn promote_new_certificate(&self) -> std::io::Result<()> {
        tokio::fs::rename(&self.new_cert_path, &self.active_cert_path).await
    }
}
