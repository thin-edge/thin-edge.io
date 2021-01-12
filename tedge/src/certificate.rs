use super::command::Command;
use chrono::offset::Utc;
use chrono::Duration;
use rcgen::CertificateParams;
use rcgen::{Certificate, RcgenError};
use std::error::Error;
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

    fn run(&self, _verbose: u8) -> Result<(), Box<dyn Error>> {
        match self {
            CertCmd::Create {
                id,
                cert_path,
                key_path,
            } => create_test_certificate(id, cert_path, key_path)?,
            _ => {
                println!("Not implemented {:?}", self);
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
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|err| err.into())
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
