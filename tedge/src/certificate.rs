use super::command::Cmd;
use chrono::offset::Utc;
use chrono::Duration;
use rcgen::Certificate;
use rcgen::CertificateParams;
use std::error::Error;
use std::fs::File;
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

impl Cmd for CertCmd {
    fn run(&self, _verbose: u8) -> Result<(), Box<dyn Error>> {
        match self {
            CertCmd::Create {
                id,
                cert_path,
                key_path,
            } => create_test_certificate(id, cert_path, key_path),
            _ => {
                println!("Not implemented {:?}", self);
                Ok(())
            }
        }
    }
}

fn create_test_certificate(
    id: &str,
    cert_path: &str,
    key_path: &str,
) -> Result<(), Box<dyn Error>> {

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

    let cert = Certificate::from_params(params)?;

    let cert_pem = cert.serialize_pem()?;
    let mut cert_file = File::create(cert_path)?;
    cert_file.write_all(cert_pem.as_bytes())?;

    let cert_key = cert.serialize_private_key_pem();
    let mut key_file = File::create(key_path)?;
    key_file.write_all(cert_key.as_bytes())?;

    Ok(())
}
