use super::create::cn_of_self_signed_certificate;
use super::error::CertError;
use crate::bridge::BridgeLocation;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::CreateCertCmd;
use camino::Utf8PathBuf;
use certificate::NewCertificateConfig;

pub struct CreateCsrCmd {
    pub id: Option<String>,
    pub cert_path: Utf8PathBuf,
    pub key_path: Utf8PathBuf,
    pub csr_path: Utf8PathBuf,
    pub bridge_location: BridgeLocation,
}

impl Command for CreateCsrCmd {
    fn description(&self) -> String {
        "Generate a Certificate Signing Request.".into()
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = NewCertificateConfig::default();
        self.create_certificate_signing_request(&config)?;
        eprintln!("Certificate Signing Request was successfully created.");
        Ok(())
    }
}

impl CreateCsrCmd {
    pub fn create_certificate_signing_request(
        &self,
        config: &NewCertificateConfig,
    ) -> Result<(), CertError> {
        // Use id of public certificate if not provided
        let id = match &self.id {
            Some(id) => id.clone(),
            None => cn_of_self_signed_certificate(&self.cert_path)?,
        };

        let create_cmd = CreateCertCmd {
            id,
            cert_path: self.csr_path.clone(),
            key_path: self.key_path.clone(),
            bridge_location: self.bridge_location,
        };

        create_cmd.create_certificate_signing_request(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CreateCertCmd;
    use assert_matches::assert_matches;
    use std::path::Path;
    use tempfile::*;
    use x509_parser::der_parser::asn1_rs::FromDer;
    use x509_parser::nom::AsBytes;

    #[test]
    fn create_signing_request_when_private_key_does_not_exist() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let csr_path = temp_file_path(&dir, "my-device-csr.csr");
        let id = "my-device-id";

        let cmd = CreateCsrCmd {
            id: Some(String::from(id)),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            csr_path: csr_path.clone(),
            bridge_location: BridgeLocation::Mosquitto,
        };

        assert_matches!(
            cmd.create_certificate_signing_request(&NewCertificateConfig::default()),
            Ok(())
        );

        assert_eq!(parse_pem_file(&csr_path).label, "CERTIFICATE REQUEST");
        assert_eq!(parse_pem_file(&key_path).label, "PRIVATE KEY");
    }

    #[test]
    fn create_signing_request_when_both_private_key_and_public_cert_exist() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let csr_path = temp_file_path(&dir, "my-device-csr.csr");
        let id = "my-device-id";

        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            bridge_location: BridgeLocation::Mosquitto,
        };

        // create private key and public cert with standard command
        assert_matches!(
            cmd.create_test_certificate(&NewCertificateConfig::default()),
            Ok(())
        );

        // Keep the cert and key data for validation
        let first_key = parse_pem_file(&key_path);
        let first_pem = parse_pem_file(&cert_path);
        let first_x509_cert = first_pem.parse_x509().expect("X.509: decoding DER failed");

        let cmd = CreateCsrCmd {
            id: Some(String::from(id)),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            csr_path: csr_path.clone(),
            bridge_location: BridgeLocation::Mosquitto,
        };

        // create csr using existing private key and device_id from public cert
        assert_matches!(
            cmd.create_certificate_signing_request(&NewCertificateConfig::default()),
            Ok(())
        );

        // Get the csr and key data for validation
        let second_key = parse_pem_file(&key_path);
        let csr_pem = parse_pem_file(&csr_path);
        let csr_subject = get_subject_from_csr(csr_pem.contents);

        // Check that private key remained the same
        assert_eq!(first_key.contents, second_key.contents);

        // Check if subject is the same
        assert_eq!(csr_subject, first_x509_cert.subject.to_string());
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> Utf8PathBuf {
        dir.path().join(filename).try_into().unwrap()
    }

    fn parse_pem_file(path: impl AsRef<Path>) -> x509_parser::pem::Pem {
        let content = std::fs::read(path).expect("fail to read {path}");

        x509_parser::pem::Pem::iter_from_buffer(&content)
            .next()
            .unwrap()
            .expect("Reading PEM block failed")
    }

    fn get_subject_from_csr(content: Vec<u8>) -> String {
        x509_parser::certification_request::X509CertificationRequest::from_der(content.as_bytes())
            .unwrap()
            .1
            .certification_request_info
            .subject
            .to_string()
    }
}
