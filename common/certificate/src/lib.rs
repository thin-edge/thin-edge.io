use chrono::offset::Utc;
use chrono::Duration;
use rcgen::Certificate;
use rcgen::CertificateParams;
use rcgen::RcgenError;
use sha1::{Digest, Sha1};
use zeroize::Zeroizing;

pub struct PemCertificate {
    pem: x509_parser::pem::Pem,
}

impl PemCertificate {
    pub fn from_pem_file(path: &str) -> Result<PemCertificate, CertificateError> {
        let file = std::fs::File::open(path)?;
        let (pem, _) = x509_parser::pem::Pem::read(std::io::BufReader::new(file))?;
        Ok(PemCertificate { pem })
    }

    pub fn from_pem_string(content: &str) -> Result<PemCertificate, CertificateError> {
        let file = std::io::Cursor::new(content.as_bytes());
        let (pem, _) = x509_parser::pem::Pem::read(std::io::BufReader::new(file))?;
        Ok(PemCertificate { pem })
    }

    pub fn subject(&self) -> Result<String, CertificateError> {
        let x509 = PemCertificate::extract_certificate(&self.pem)?;
        Ok(x509.tbs_certificate.subject.to_string())
    }

    pub fn issuer(&self) -> Result<String, CertificateError> {
        let x509 = PemCertificate::extract_certificate(&self.pem)?;
        Ok(x509.tbs_certificate.issuer.to_string())
    }

    pub fn not_before(&self) -> Result<String, CertificateError> {
        let x509 = PemCertificate::extract_certificate(&self.pem)?;
        Ok(x509.tbs_certificate.validity.not_before.to_rfc2822())
    }

    pub fn not_after(&self) -> Result<String, CertificateError> {
        let x509 = PemCertificate::extract_certificate(&self.pem)?;
        Ok(x509.tbs_certificate.validity.not_after.to_rfc2822())
    }

    pub fn thumbprint(&self) -> Result<String, CertificateError> {
        let bytes = Sha1::digest(&self.pem.contents).as_slice().to_vec();
        let strs: Vec<String> = bytes.iter().map(|b| format!("{:02X}", b)).collect();
        Ok(strs.concat())
    }

    fn extract_certificate(
        pem: &x509_parser::pem::Pem,
    ) -> Result<x509_parser::certificate::X509Certificate, CertificateError> {
        let x509 = pem.parse_x509().map_err(|err| {
            // The x509 error is wrapped into a `nom::Err`
            // and cannot be extracted without pattern matching on that type
            // So one simply extract the error as a string,
            // to avoid a dependency on the `nom` crate.
            let x509_error_string = format!("{}", err);
            CertificateError::X509Error(x509_error_string)
        })?;
        Ok(x509)
    }
}

pub struct KeyCertPair {
    certificate: rcgen::Certificate,
}

impl KeyCertPair {
    pub fn new_selfsigned_certificate(
        config: &NewCertificateConfig,
        id: &str,
    ) -> Result<KeyCertPair, CertificateError> {
        let () = KeyCertPair::check_identifier(id, config.max_cn_size)?;
        let mut distinguished_name = rcgen::DistinguishedName::new();
        distinguished_name.push(rcgen::DnType::CommonName, id);
        distinguished_name.push(rcgen::DnType::OrganizationName, &config.organization_name);
        distinguished_name.push(
            rcgen::DnType::OrganizationalUnitName,
            &config.organizational_unit_name,
        );

        let today = Utc::now();
        let not_before = today - Duration::days(1); // Ensure the certificate is valid today
        let not_after = today + Duration::days(config.validity_period_days.into());

        let mut params = CertificateParams::default();
        params.distinguished_name = distinguished_name;
        params.not_before = not_before;
        params.not_after = not_after;
        params.alg = &rcgen::PKCS_ECDSA_P256_SHA256; // ECDSA signing using the P-256 curves and SHA-256 hashing as per RFC 5758
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained); // IsCa::SelfSignedOnly is rejected by C8Y

        Ok(KeyCertPair {
            certificate: Certificate::from_params(params)?,
        })
    }

    pub fn certificate_pem_string(&self) -> Result<String, CertificateError> {
        Ok(self.certificate.serialize_pem()?)
    }

    pub fn private_key_pem_string(&self) -> Result<Zeroizing<String>, CertificateError> {
        Ok(Zeroizing::new(self.certificate.serialize_private_key_pem()))
    }

    fn check_identifier(id: &str, max_cn_size: usize) -> Result<(), CertificateError> {
        if id.is_empty() {
            return Err(CertificateError::EmptyName);
        } else if id.len() > max_cn_size {
            return Err(CertificateError::TooLongName {
                name: id.into(),
                max_cn_size,
            });
        } else if id.contains(char::is_control) {
            return Err(CertificateError::InvalidCharacter { name: id.into() });
        }

        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CertificateError {
    #[error(r#"The string '{name:?}' contains characters which cannot be used in a name"#)]
    InvalidCharacter { name: String },

    #[error(r#"The empty string cannot be used as a name"#)]
    EmptyName,

    #[error(
    r#"The string '{name:?}' is more than {max_cn_size} characters long and cannot be used as a name"#
    )]
    TooLongName { name: String, max_cn_size: usize },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Cryptography related error")]
    CryptographyError(#[from] RcgenError),

    #[error("PEM file format error")]
    PemError(#[from] x509_parser::error::PEMError),

    #[error("X509 file format error: {0}")]
    X509Error(String), // One cannot use x509_parser::error::X509Error unless one use `nom`.
}

pub struct NewCertificateConfig {
    pub max_cn_size: usize,
    pub validity_period_days: u32,
    pub organization_name: String,
    pub organizational_unit_name: String,
}

impl Default for NewCertificateConfig {
    fn default() -> Self {
        NewCertificateConfig {
            max_cn_size: 64,
            validity_period_days: 365,
            organization_name: "Thin Edge".into(),
            organizational_unit_name: "Test Device".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_certificate_thumbprint_b64_decode_sha1() {
        // Create a certificate key pair
        let id = "my-device-id";
        let config = NewCertificateConfig::default();
        let keypair = KeyCertPair::new_selfsigned_certificate(&config, id)
            .expect("Fail to create a certificate");

        // Read the certificate pem
        let pem_string = keypair
            .certificate_pem_string()
            .expect("Fail to read the certificate PEM");
        let pem = PemCertificate::from_pem_string(&pem_string)
            .expect("Fail to decode the certificate PEM");

        // Compute the thumbprint of the certificate using this crate
        let thumbprint = pem.thumbprint().expect("Fail to compute the thumbprint");

        // Compute the expected thumbprint of the certificate using base64 and sha1
        // Remove new line and carriage return characters
        let cert_cont = pem_string.replace(&['\r', '\n'][..], "");

        // Read the certificate contents, except the header and footer
        let header_len = "-----BEGIN CERTIFICATE-----".len();
        let footer_len = "-----END CERTIFICATE-----".len();

        // just decode the key contents
        let b64_bytes =
            base64::decode(&cert_cont[header_len..cert_cont.len() - footer_len]).unwrap();
        let expected_thumbprint = format!("{:x}", sha1::Sha1::digest(b64_bytes.as_ref()));

        // compare the two thumbprints
        assert_eq!(thumbprint, expected_thumbprint.to_uppercase());
    }

    #[test]
    fn check_thumbprint_static_certificate() {
        let cert_content = r#"-----BEGIN CERTIFICATE-----
MIIBlzCCAT2gAwIBAgIBKjAKBggqhkjOPQQDAjA7MQ8wDQYDVQQDDAZteS10YnIx
EjAQBgNVBAoMCVRoaW4gRWRnZTEUMBIGA1UECwwLVGVzdCBEZXZpY2UwHhcNMjEw
MzA5MTQxMDMwWhcNMjIwMzEwMTQxMDMwWjA7MQ8wDQYDVQQDDAZteS10YnIxEjAQ
BgNVBAoMCVRoaW4gRWRnZTEUMBIGA1UECwwLVGVzdCBEZXZpY2UwWTATBgcqhkjO
PQIBBggqhkjOPQMBBwNCAAR6DVDOQ9ey3TX4tD2V0zCYe8GtmUHekNZZX6P+lUXx
886P/Kkyra0xCYKam2me2VzdLMc4X5cpRkybVa0XH/WCozIwMDAdBgNVHQ4EFgQU
Iz8LzGgzHjqsvB+ppPsVa+xf2bYwDwYDVR0TAQH/BAUwAwEB/zAKBggqhkjOPQQD
AgNIADBFAiEAhMAATBcZqE3Li1TZCzDoweBxRw1WD6gaSAcrsIWuW94CIHuR5ZG7
ozYxD+f5npF5kWWKcLIIo0wqvXg0GOLNfxTh
-----END CERTIFICATE-----
"#;
        let expected_thumbprint = "860218AD0A996004449521E2713C28F67B5EA580";

        let pem = PemCertificate::from_pem_string(cert_content).expect("Reading PEM failed");
        let thumbprint = pem.thumbprint().expect("Extracting thumbprint failed");
        assert_eq!(thumbprint, expected_thumbprint);
    }
}
