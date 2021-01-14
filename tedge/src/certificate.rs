use super::command::Command;
use chrono::offset::Utc;
use chrono::Duration;
use rcgen::CertificateParams;
use rcgen::{Certificate, RcgenError};
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum CertCmd {
    /// Create a device certificate
    Create {
        /// The device identifier
        #[structopt(long)]
        id: String,

        /// The path where the device certificate will be stored
        #[structopt(long, default_value = "./tedge-certificate.pem")]
        cert_path: String,

        /// The path where the device private key will be stored
        #[structopt(long, default_value = "./tedge-private-key.pem")]
        key_path: String,
    },

    /// Show the device certificate, if any
    Show,

    /// Remove the device certificate
    Remove,
}

#[derive(thiserror::Error, Debug)]
pub enum CertError {
    #[error(
        r#"A certificate already exists and would be overwritten.
       Run `tegde cert remove` first to generate a new certificate.
    "#
    )]
    AlreadyExists,

    #[error("I/O error")]
    IoError(std::io::Error),

    #[error("Cryptography related error")]
    PemError(#[from] RcgenError),
}

impl From<std::io::Error> for CertError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::AlreadyExists => CertError::AlreadyExists,
            _ => CertError::IoError(err),
        }
    }
}

impl Command for CertCmd {
    fn to_string(&self) -> String {
        match self {
            CertCmd::Create {
                id,
                cert_path: _,
                key_path: _,
            } => format!("create a test certificate for the device {}.", id),
            CertCmd::Show => format!("show the device certificate"),
            CertCmd::Remove => format!("remove the device certificate"),
        }
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        match self {
            CertCmd::Create {
                id,
                cert_path,
                key_path,
            } => create_test_certificate(id, cert_path, key_path)?,
            _ => {
                unimplemented!("{:?}", self);
            }
        }
        Ok(())
    }
}

fn create_test_certificate(id: &str, cert_path: &str, key_path: &str) -> Result<(), CertError> {
    let mut cert_file = create_new_file(cert_path)?;
    let mut key_file = create_new_file(key_path)?;

    let cert = new_selfsigned_certificate(id)?;

    let cert_pem = cert.serialize_pem()?;
    cert_file.write_all(cert_pem.as_bytes())?;

    let cert_key = cert.serialize_private_key_pem();
    key_file.write_all(cert_key.as_bytes())?;

    Ok(())
}

fn create_new_file(path: &str) -> Result<File, CertError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn new_selfsigned_certificate(id: &str) -> Result<Certificate, RcgenError> {
    let mut distinguished_name = rcgen::DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, id);
    distinguished_name.push(rcgen::DnType::OrganizationName, "Thin Edge");
    distinguished_name.push(rcgen::DnType::OrganizationalUnitName, "Test Device");

    let today = Utc::now();
    let not_before = today - Duration::days(1); // Ensure the certificate is valid today
    let not_after = today + Duration::days(90);

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    params.not_before = not_before;
    params.not_after = not_after;
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256; // ECDSA signing using the P-256 curves and SHA-256 hashing as per RFC 5758
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained); // IsCa::SelfSignedOnly is rejected by C8Y

    Certificate::from_params(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::*;

    #[test]
    fn basic_usage() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let id = "my-device-id";

        let cmd = CertCmd::Create {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };
        let verbose = 0;

        assert!(cmd.run(verbose).err().is_none());
        assert_eq!(parse_pem_file(&cert_path).unwrap().tag, "CERTIFICATE");
        assert_eq!(parse_pem_file(&key_path).unwrap().tag, "PRIVATE KEY");
    }

    #[test]
    fn check_certificate_is_not_overwritten() {
        let cert_content = "some cert content";
        let key_content = "some key content";
        let cert_file = temp_file_with_content(cert_content);
        let key_file = temp_file_with_content(key_content);
        let id = "my-device-id";

        let cmd = CertCmd::Create {
            id: String::from(id),
            cert_path: String::from(cert_file.path().to_str().unwrap()),
            key_path: String::from(key_file.path().to_str().unwrap()),
        };
        let verbose = 0;

        assert!(cmd.run(verbose).ok().is_none());

        let mut cert_file = cert_file.reopen().unwrap();
        assert_eq!(file_content(&mut cert_file), cert_content);

        let mut key_file = key_file.reopen().unwrap();
        assert_eq!(file_content(&mut key_file), key_content);
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }

    fn file_content(file: &mut File) -> String {
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        content
    }

    fn parse_pem_file(path: &str) -> Result<pem::Pem, String> {
        let mut file = File::open(path).map_err(|err| err.to_string())?;
        let content = file_content(&mut file);

        pem::parse(content).map_err(|err| err.to_string())
    }
}
