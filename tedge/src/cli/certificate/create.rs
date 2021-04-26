use super::error::CertError;
use super::file_installer::*;
use crate::command::{Command, ExecutionContext};
use crate::system_command::Role;
use certificate::{KeyCertPair, NewCertificateConfig};
use tedge_config::*;

/// Create a self-signed device certificate
pub struct CreateCertCmd {
    /// The device identifier
    pub id: String,

    /// The path where the device certificate will be stored
    pub cert_path: FilePath,

    /// The path where the device private key will be stored
    pub key_path: FilePath,
}

impl Command for CreateCertCmd {
    fn description(&self) -> String {
        format!("create a test certificate for the device {}.", self.id)
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let config = NewCertificateConfig::default();
        let () = self.create_test_certificate(&config, &Installer)?;
        Ok(())
    }
}

impl CreateCertCmd {
    fn create_test_certificate(
        &self,
        config: &NewCertificateConfig,
        installer: &dyn FileInstaller,
    ) -> Result<(), CertError> {
        let cert = KeyCertPair::new_selfsigned_certificate(&config, &self.id)?;
        let cert_pem = cert.certificate_pem_string()?;
        let cert_key = cert.private_key_pem_string()?;

        // 0o444: Prevents the certificate to be overwritten
        let () = installer
            .install(
                self.cert_path.as_ref(),
                Role::Broker,
                0o444,
                cert_pem.as_bytes(),
            )
            .map_err(|err| err.cert_context(self.cert_path.clone()))?;

        // 0o600: Make sure the key is secret and cannot be written
        let () = installer
            .install(
                self.key_path.as_ref(),
                Role::Broker,
                0o400,
                cert_key.as_bytes(),
            )
            .map_err(|err| err.cert_context(self.key_path.clone()))?;

        Ok(())
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
    fn basic_usage() {
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
            cmd.create_test_certificate(&NewCertificateConfig::default(), &Installer),
            Ok(())
        );
        assert_eq!(parse_pem_file(&cert_path).unwrap().tag, "CERTIFICATE");
        assert_eq!(parse_pem_file(&key_path).unwrap().tag, "PRIVATE KEY");
    }

    #[test]
    fn check_certificate_is_not_overwritten() {
        let dir = tempdir().unwrap();

        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");

        let cert_content = "some cert content";
        let key_content = "some key content";

        fs::write(&cert_path, cert_content).unwrap();
        fs::write(&key_path, key_content).unwrap();

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
        };

        assert!(cmd
            .create_test_certificate(&NewCertificateConfig::default(), &Installer)
            .ok()
            .is_none());

        assert_eq!(fs::read(&cert_path).unwrap(), cert_content.as_bytes());
        assert_eq!(fs::read(&key_path).unwrap(), key_content.as_bytes());
    }

    #[test]
    fn create_certificate_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let cert_path = FilePath::from("/non/existent/cert/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default(), &Installer)
            .unwrap_err();
        assert_matches!(cert_error, CertError::CertPathError { .. });
    }

    #[test]
    fn create_key_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = FilePath::from("/non/existent/key/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default(), &Installer)
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> FilePath {
        dir.path().join(filename).into()
    }

    fn parse_pem_file(path: impl AsRef<Path>) -> Result<pem::Pem, String> {
        let content = fs::read(path).map_err(|err| err.to_string())?;
        pem::parse(content).map_err(|err| err.to_string())
    }
}
