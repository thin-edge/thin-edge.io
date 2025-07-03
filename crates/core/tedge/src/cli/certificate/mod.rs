use crate::cli::certificate::create_csr::CreateCsrCmd;

pub use self::cli::TEdgeCertCli;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use tokio::io::AsyncReadExt;

mod c8y;
mod cli;
mod create;
mod create_csr;
mod create_key;
mod error;
mod remove;
mod renew;
mod shift;
mod show;

pub use self::cli::*;
pub use self::create::*;
pub use self::error::*;
pub use self::shift::*;

pub(crate) async fn read_cert_to_string(path: impl AsRef<Utf8Path>) -> Result<String, CertError> {
    let mut file = tokio::fs::File::open(path.as_ref()).await.map_err(|err| {
        CertError::CertificateIoError {
            source: err,
            path: path.as_ref().to_owned(),
        }
    })?;
    let mut content = String::new();
    file.read_to_string(&mut content).await?;

    Ok(content)
}

/// Create a device private key and CSR
///
/// Return the CSR in the format expected by c8y CA
async fn create_device_csr(
    common_name: String,
    key: create_csr::Key,
    current_cert: Option<Utf8PathBuf>,
    csr_path: Utf8PathBuf,
    csr_template: CsrTemplate,
) -> Result<(), CertError> {
    let create_cmd = CreateCsrCmd {
        id: common_name,
        csr_path: csr_path.clone(),
        key,
        current_cert,
        user: "tedge".to_string(),
        group: "tedge".to_string(),
        csr_template,
    };
    create_cmd.create_certificate_signing_request().await?;
    Ok(())
}

#[cfg(test)]
mod test_helpers {
    use camino::Utf8PathBuf;
    use std::path::Path;
    use tempfile::TempDir;
    use x509_parser::der_parser::asn1_rs::FromDer;
    use x509_parser::nom::AsBytes;

    pub fn temp_file_path(dir: &TempDir, filename: &str) -> Utf8PathBuf {
        dir.path().join(filename).try_into().unwrap()
    }
    pub fn parse_pem_file(path: impl AsRef<Path>) -> pem::Pem {
        let content = std::fs::read(path).expect("fail to read {path}");
        pem::parse(content).expect("Reading PEM block failed")
    }

    pub fn parse_x509_file(path: impl AsRef<Path>) -> x509_parser::pem::Pem {
        let content = std::fs::read(path).expect("fail to read {path}");

        x509_parser::pem::Pem::iter_from_buffer(&content)
            .next()
            .unwrap()
            .expect("Reading PEM block failed")
    }

    pub fn get_subject_from_csr(content: Vec<u8>) -> String {
        x509_parser::certification_request::X509CertificationRequest::from_der(content.as_bytes())
            .unwrap()
            .1
            .certification_request_info
            .subject
            .to_string()
    }
}
