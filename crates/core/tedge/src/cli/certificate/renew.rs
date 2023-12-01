use super::error::CertError;
use super::remove::RemoveCertCmd;
use crate::command::Command;
use crate::CreateCertCmd;
use camino::Utf8PathBuf;
use certificate::NewCertificateConfig;
use certificate::PemCertificate;

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
        let id = self.cn_of_self_signed_certificate()?;
        // Remove existing certificate
        let rm_cmd = RemoveCertCmd {
            cert_path: self.cert_path.clone(),
            key_path: self.key_path.clone(),
        };
        rm_cmd.remove_certificate()?;

        // Re-create the certificate, with new validity
        let create_cmd = CreateCertCmd {
            id,
            cert_path: self.cert_path.clone(),
            key_path: self.key_path.clone(),
        };
        create_cmd.create_test_certificate(config)
    }

    fn cn_of_self_signed_certificate(&self) -> Result<String, CertError> {
        let pem = PemCertificate::from_pem_file(&self.cert_path).map_err(|err| match err {
            certificate::CertificateError::IoError(from) => {
                CertError::IoError(from).cert_context(self.cert_path.clone())
            }
            from => CertError::CertificateError(from),
        })?;

        if pem.issuer()? == pem.subject()? {
            Ok(pem.subject_common_name()?)
        } else {
            Err(CertError::NotASelfSignedCertificate {
                path: self.cert_path.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::fs;
    use std::path::Path;
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
        };

        assert_matches!(
            cmd.create_test_certificate(&NewCertificateConfig::default()),
            Ok(())
        );

        let cmd = RenewCertCmd {
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };

        assert_matches!(
            cmd.renew_test_certificate(&NewCertificateConfig::default()),
            Ok(())
        );
        assert_eq!(parse_pem_file(&cert_path).unwrap().tag, "CERTIFICATE");
        assert_eq!(parse_pem_file(&key_path).unwrap().tag, "PRIVATE KEY");
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> Utf8PathBuf {
        dir.path().join(filename).try_into().unwrap()
    }

    fn parse_pem_file(path: impl AsRef<Path>) -> Result<pem::Pem, String> {
        let content = fs::read(path).map_err(|err| err.to_string())?;
        pem::parse(content).map_err(|err| err.to_string())
    }
}
