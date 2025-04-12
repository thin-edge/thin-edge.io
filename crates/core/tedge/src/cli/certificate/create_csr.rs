use super::error::CertError;
use crate::command::Command;
use crate::log::MaybeFancy;
use crate::override_public_key;
use crate::persist_new_private_key;
use crate::reuse_private_key;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
use certificate::KeyCertPair;
use certificate::KeyKind;

/// Create a certificate signing request (CSR)
pub struct CreateCsrCmd {
    /// The device identifier
    pub id: String,

    /// The path where the device private key will be stored
    pub key_path: Utf8PathBuf,

    /// The path where the device CSR will be stored
    pub csr_path: Utf8PathBuf,

    /// The owner of the private key
    pub user: String,
    pub group: String,

    /// CSR template
    pub csr_template: CsrTemplate,
}

#[async_trait::async_trait]
impl Command for CreateCsrCmd {
    fn description(&self) -> String {
        "Generate a Certificate Signing Request.".into()
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.create_certificate_signing_request().await?;
        eprintln!("Certificate Signing Request was successfully created.");
        Ok(())
    }
}

impl CreateCsrCmd {
    pub async fn create_certificate_signing_request(&self) -> Result<(), CertError> {
        let id = &self.id;
        let csr_path = tedge_api::path::resolve_to_absolute_utf8_path(&self.csr_path)?;
        let key_path = &self.key_path;

        let previous_key = reuse_private_key(key_path)
            .await
            .map_err(|e| CertError::IoError(e).key_context(key_path.clone()))?;

        let cert =
            KeyCertPair::new_certificate_sign_request(&self.csr_template, id, &previous_key)?;

        if let KeyKind::New = previous_key {
            persist_new_private_key(
                key_path,
                cert.private_key_pem_string()?,
                &self.user,
                &self.group,
            )
            .await
            .map_err(|err| err.key_context(key_path.clone()))?;
        }
        override_public_key(&csr_path, cert.certificate_signing_request_string()?)
            .await
            .map_err(|err| err.cert_context(csr_path.clone()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::certificate::test_helpers::*;
    use crate::CreateCertCmd;
    use assert_matches::assert_matches;
    use tempfile::*;

    #[tokio::test]
    async fn create_signing_request_when_private_key_does_not_exist() {
        let dir = tempdir().unwrap();
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let csr_path = temp_file_path(&dir, "my-device-csr.csr");
        let id = "my-device-id";

        let cmd = CreateCsrCmd {
            id: id.to_string(),
            key_path: key_path.clone(),
            csr_path: csr_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            csr_template: CsrTemplate::default(),
        };

        assert_matches!(cmd.create_certificate_signing_request().await, Ok(()));

        assert_eq!(parse_x509_file(&csr_path).label, "CERTIFICATE REQUEST");
        assert_eq!(parse_x509_file(&key_path).label, "PRIVATE KEY");
    }

    #[tokio::test]
    async fn create_signing_request_when_both_private_key_and_public_cert_exist() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let csr_path = temp_file_path(&dir, "my-device-csr.csr");
        let id = "my-device-id";

        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            csr_template: CsrTemplate::default(),
        };

        // create private key and public cert with standard command
        assert_matches!(
            cmd.create_test_certificate(&CsrTemplate::default()).await,
            Ok(())
        );

        // Keep the cert and key data for validation
        let first_key = parse_x509_file(&key_path);
        let first_pem = parse_x509_file(&cert_path);
        let first_x509_cert = first_pem.parse_x509().expect("X.509: decoding DER failed");

        let cmd = CreateCsrCmd {
            id: id.to_string(),
            key_path: key_path.clone(),
            csr_path: csr_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            csr_template: CsrTemplate::default(),
        };

        // create csr using existing private key and device_id from public cert
        assert_matches!(cmd.create_certificate_signing_request().await, Ok(()));

        // Get the csr and key data for validation
        let second_key = parse_x509_file(&key_path);
        let csr_pem = parse_x509_file(&csr_path);
        let csr_subject = get_subject_from_csr(csr_pem.contents);

        // Check that private key remained the same
        assert_eq!(first_key.contents, second_key.contents);

        // Check if subject is the same
        assert_eq!(csr_subject, first_x509_cert.subject.to_string());
    }
}
