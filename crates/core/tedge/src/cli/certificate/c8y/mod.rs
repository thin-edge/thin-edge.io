mod download;
mod renew;
mod upload;

use crate::cli::certificate::create_csr::CreateCsrCmd;
use crate::read_cert_to_string;
use crate::CertError;
use camino::Utf8PathBuf;
use certificate::NewCertificateConfig;
pub use download::DownloadCertCmd;
pub use renew::RenewCertCmd;
use std::fs::OpenOptions;
use std::io::Write;
use tedge_utils::paths::set_permission;
pub use upload::UploadCertCmd;

/// Create a device private key and CSR
fn create_device_csr(
    common_name: String,
    key_path: Utf8PathBuf,
    csr_path: Utf8PathBuf,
) -> Result<String, CertError> {
    let config = NewCertificateConfig::default();
    let create_cmd = CreateCsrCmd {
        id: common_name,
        csr_path: csr_path.clone(),
        key_path,
        user: "tedge".to_string(),
        group: "tedge".to_string(),
    };
    create_cmd.create_certificate_signing_request(&config)?;
    read_cert_to_string(&csr_path)
}

/// Store a device certificate
fn store_device_cert(cert_path: &Utf8PathBuf, cert: String) -> Result<(), CertError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(cert_path)?;

    file.write_all(cert.as_bytes())?;
    file.sync_all()?;

    set_permission(&file, 0o444)?;
    Ok(())
}
