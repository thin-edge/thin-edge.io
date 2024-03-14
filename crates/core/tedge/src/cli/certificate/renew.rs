use super::create::cn_of_self_signed_certificate;
use super::error::CertError;
use crate::command::Command;
use crate::CreateCertCmd;
use camino::Utf8PathBuf;
use certificate::NewCertificateConfig;

pub struct RenewCertCmd {
    pub cert_path: Utf8PathBuf,
    pub key_path: Utf8PathBuf,
}

impl Command for RenewCertCmd {
    fn description(&self) -> String {
        "Renew the self-signed certificate of the device.".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        let config = NewCertificateConfig::default();
        self.renew_test_certificate(&config)?;
        eprintln!("Certificate was successfully renewed, for un-interrupted service, the certificate has to be uploaded to the cloud");
        Ok(())
    }
}

impl RenewCertCmd {
    fn renew_test_certificate(&self, config: &NewCertificateConfig) -> Result<(), CertError> {
        let id = cn_of_self_signed_certificate(&self.cert_path)?;

        // Remove only certificate
        std::fs::remove_file(&self.cert_path)
            .map_err(|e| CertError::IoError(e).cert_context(self.cert_path.clone()))?;

        // Re-create the certificate from the key, with new validity
        let create_cmd = CreateCertCmd {
            id,
            cert_path: self.cert_path.clone(),
            key_path: self.key_path.clone(),
            csr_path: None,
        };

        create_cmd.renew_test_certificate(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::thread::sleep;
    use std::time::Duration;
    use tempfile::*;

    #[test]
    fn validate_renew_certificate() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let id = "my-device-id";
        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            csr_path: None,
        };

        // First create both cert and key
        cmd.create_test_certificate(&NewCertificateConfig::default())
            .unwrap();

        // Keep the cert and key data for validation
        let first_key = std::fs::read_to_string(&key_path).unwrap();
        let first_pem = parse_pem_file(&cert_path);
        let first_x509_cert = first_pem.parse_x509().expect("X.509: decoding DER failed");

        // Wait 2 secs to get different timestamp for the certificate validity
        sleep(Duration::from_secs(2));

        // Renew the certificate from the existing key
        let cmd = RenewCertCmd {
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };
        cmd.renew_test_certificate(&NewCertificateConfig::default())
            .unwrap();

        // Get the cert and key data for validation
        let second_key = std::fs::read_to_string(&key_path).unwrap();
        let second_pem = parse_pem_file(&cert_path);
        let second_x509_cert = second_pem.parse_x509().expect("X.509: decoding DER failed");

        // The key must be unchanged
        assert_eq!(first_key, second_key);

        // The new cert must have newer validity than the first one
        assert!(
            second_x509_cert.validity.not_before.timestamp()
                > first_x509_cert.validity.not_before.timestamp()
        );

        // The renewed cert is issued by thin-edge
        assert_eq!(
            second_x509_cert.issuer().to_string(),
            "CN=my-device-id, O=Thin Edge, OU=Test Device"
        );
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
}
