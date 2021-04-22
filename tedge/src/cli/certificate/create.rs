use crate::command::{Command, ExecutionContext};
use crate::utils::{paths, users};
use certificate::{KeyCertPair, NewCertificateConfig};
use std::{
    fs::{File, OpenOptions},
    io::prelude::*,
    path::Path,
};
use tedge_config::*;

use super::error::CertError;

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

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let config = NewCertificateConfig::default();
        let () = self.create_test_certificate(&config, &context.user_manager)?;
        Ok(())
    }
}

impl CreateCertCmd {
    fn create_test_certificate(
        &self,
        config: &NewCertificateConfig,
        user_manager: &users::UserManager,
    ) -> Result<(), CertError> {
        let _user_guard = user_manager.become_user(users::BROKER_USER)?;

        paths::validate_parent_dir_exists(&self.cert_path).map_err(CertError::CertPathError)?;
        paths::validate_parent_dir_exists(&self.key_path).map_err(CertError::KeyPathError)?;

        // Creating files with permission 644
        let mut cert_file = create_new_file(&self.cert_path)
            .map_err(|err| err.cert_context(self.cert_path.clone()))?;
        let mut key_file = create_new_file(&self.key_path)
            .map_err(|err| err.key_context(self.key_path.clone()))?;

        let cert = KeyCertPair::new_selfsigned_certificate(&config, &self.id)?;

        let cert_pem = cert.certificate_pem_string()?;
        cert_file.write_all(cert_pem.as_bytes())?;
        cert_file.sync_all()?;

        // Prevent the certificate to be overwritten
        paths::set_permission(&cert_file, 0o444)?;

        {
            // Make sure the key is secret, before write
            paths::set_permission(&key_file, 0o600)?;

            // Zero the private key on drop
            let cert_key = cert.private_key_pem_string()?;
            key_file.write_all(cert_key.as_bytes())?;
            key_file.sync_all()?;

            // Prevent the key to be overwritten
            paths::set_permission(&key_file, 0o400)?;
        }

        Ok(())
    }
}

fn create_new_file(path: impl AsRef<Path>) -> Result<File, CertError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::users::UserManager;
    use assert_matches::assert_matches;
    use std::fs;
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
            cmd.create_test_certificate(&NewCertificateConfig::default(), &UserManager::new()),
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
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
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
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
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
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
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
