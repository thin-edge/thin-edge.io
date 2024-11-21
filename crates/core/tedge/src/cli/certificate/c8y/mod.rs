mod download;
mod upload;

use crate::cli::certificate::create_csr::CreateCsrCmd;
use crate::override_public_key;
use crate::read_cert_to_string;
use crate::CertError;
use camino::Utf8PathBuf;
use certificate::NewCertificateConfig;
pub use download::DownloadCertCmd;
pub use upload::UploadCertCmd;

/// Create a device private key and CSR
///
/// Return the CSR in the format expected by c8y CA
async fn create_device_csr(
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
    create_cmd
        .create_certificate_signing_request(&config)
        .await?;

    let csr = read_cert_to_string(&csr_path)?;
    let csr = csr
        .strip_prefix("-----BEGIN CERTIFICATE REQUEST-----\n")
        .unwrap_or(&csr);
    let csr = csr
        .strip_suffix("-----END CERTIFICATE REQUEST-----\n")
        .unwrap_or(csr)
        .to_string();
    Ok(csr)
}

/// Store a device certificate received from c8y CA
///
/// Notably this adds PEM header and trailer.
async fn store_device_cert(cert_path: &Utf8PathBuf, cert: String) -> Result<(), CertError> {
    let pem_string =
        String::new() + "-----BEGIN CERTIFICATE-----\n" + &cert + "\n-----END CERTIFICATE-----\n";
    override_public_key(cert_path, pem_string).await
}
