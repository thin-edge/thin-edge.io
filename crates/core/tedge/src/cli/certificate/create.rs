use super::error::CertError;
use crate::bridge::BridgeLocation;
use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8PathBuf;
use certificate::KeyCertPair;
use certificate::KeyKind;
use certificate::NewCertificateConfig;
use certificate::PemCertificate;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;
use tedge_utils::paths::set_permission;
use tedge_utils::paths::validate_parent_dir_exists;

/// Create a self-signed device certificate
pub struct CreateCertCmd {
    /// The device identifier
    pub id: String,

    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,

    /// The path where the device private key will be stored
    pub key_path: Utf8PathBuf,

    /// The path where the device CSR file will be stored
    pub csr_path: Option<Utf8PathBuf>,

    /// The component that is configured to host the MQTT bridge logic
    pub bridge_location: BridgeLocation,
}

impl Command for CreateCertCmd {
    fn description(&self) -> String {
        format!("create a test certificate for the device {}.", self.id)
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = NewCertificateConfig::default();
        self.create_test_certificate(&config)?;
        eprintln!("Certificate was successfully created");
        Ok(())
    }
}

impl CreateCertCmd {
    pub fn create_test_certificate(&self, config: &NewCertificateConfig) -> Result<(), CertError> {
        let cert = KeyCertPair::new_selfsigned_certificate(config, &self.id, &KeyKind::New)?;

        let cert_path = &self.cert_path;
        self.persist_new_public_key(cert_path, cert.certificate_pem_string()?)
            .map_err(|err| err.cert_context(cert_path.clone()))?;

        let key_path = &self.key_path;
        self.persist_new_private_key(key_path, cert.private_key_pem_string()?)
            .map_err(|err| err.key_context(key_path.clone()))?;
        Ok(())
    }

    pub fn renew_test_certificate(&self, config: &NewCertificateConfig) -> Result<(), CertError> {
        let cert_path = &self.cert_path;
        let key_path = &self.key_path;

        let previous_key = reuse_private_key(key_path)
            .map_err(|e| CertError::IoError(e).key_context(key_path.clone()))?;
        let cert = KeyCertPair::new_selfsigned_certificate(config, &self.id, &previous_key)?;

        self.override_public_key(cert_path, cert.certificate_pem_string()?)
            .map_err(|err| err.cert_context(cert_path.clone()))?;
        Ok(())
    }

    pub fn create_certificate_signing_request(
        &self,
        config: &NewCertificateConfig,
    ) -> Result<(), CertError> {
        // FIXME the current implementation of CreateCertCmd is ambiguous
        //       One can prepare to create a self-signed certificate and then try to create a CSR
        let csr_path = self
            .csr_path
            .as_ref()
            .expect("creating a CSR instead of a self-signed certificate");
        let key_path = &self.key_path;

        let previous_key = reuse_private_key(key_path).unwrap_or(KeyKind::New);
        let cert = KeyCertPair::new_certificate_sign_request(config, &self.id, &previous_key)?;

        if let KeyKind::New = previous_key {
            self.persist_new_private_key(key_path, cert.private_key_pem_string()?)
                .map_err(|err| err.key_context(key_path.clone()))?;
        }
        self.override_public_key(csr_path, cert.certificate_signing_request_string()?)
            .map_err(|err| err.cert_context(csr_path.clone()))?;
        Ok(())
    }

    fn persist_new_public_key(
        &self,
        cert_path: &Utf8PathBuf,
        pem_string: String,
    ) -> Result<(), CertError> {
        let (user, group) = self.certificate_user_group();

        validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
        persist_public_key(create_new_file(cert_path, user, group)?, pem_string)?;
        Ok(())
    }

    fn persist_new_private_key(
        &self,
        key_path: &Utf8PathBuf,
        key: certificate::Zeroizing<String>,
    ) -> Result<(), CertError> {
        let (user, group) = self.certificate_user_group();

        validate_parent_dir_exists(key_path).map_err(CertError::KeyPathError)?;
        persist_private_key(create_new_file(key_path, user, group)?, key)?;
        Ok(())
    }

    fn override_public_key(
        &self,
        cert_path: &Utf8PathBuf,
        pem_string: String,
    ) -> Result<(), CertError> {
        validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
        persist_public_key(override_file(cert_path)?, pem_string)?;
        Ok(())
    }

    fn certificate_user_group(&self) -> (&str, &str) {
        match self.bridge_location {
            BridgeLocation::BuiltIn => ("tedge", "tedge"),
            BridgeLocation::Mosquitto => (crate::BROKER_USER, crate::BROKER_GROUP),
        }
    }
}

fn create_new_file(
    path: impl AsRef<Path>,
    user: &str,
    group: &str,
) -> Result<File, std::io::Error> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.as_ref())?;

    // Ignore errors - This was the behavior with the now deprecated user manager.
    // - When `tedge cert create` is not run as root, a certificate is created but owned by the user running the command.
    // - A better approach could be to remove this `chown` and run the command as mosquitto.
    let _ = tedge_utils::file::change_user_and_group(path.as_ref(), user, group);

    Ok(file)
}

fn override_file(path: impl AsRef<Path>) -> Result<File, std::io::Error> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.as_ref())
}

fn reuse_private_key(key_path: &Utf8PathBuf) -> Result<KeyKind, std::io::Error> {
    std::fs::read_to_string(key_path).map(|keypair_pem| KeyKind::Reuse { keypair_pem })
}

fn persist_private_key(
    mut key_file: File,
    cert_key: certificate::Zeroizing<String>, // Zero the private key on drop
) -> Result<(), std::io::Error> {
    // Make sure the key is secret, before write
    set_permission(&key_file, 0o600)?;
    key_file.write_all(cert_key.as_bytes())?;
    key_file.sync_all()?;

    // Prevent the key to be overwritten
    set_permission(&key_file, 0o400)?;
    Ok(())
}

fn persist_public_key(mut key_file: File, cert_pem: String) -> Result<(), std::io::Error> {
    key_file.write_all(cert_pem.as_bytes())?;
    key_file.sync_all()?;

    // Make the file public
    set_permission(&key_file, 0o444)?;
    Ok(())
}

pub fn cn_of_self_signed_certificate(cert_path: &Utf8PathBuf) -> Result<String, CertError> {
    let pem = PemCertificate::from_pem_file(cert_path).map_err(|err| match err {
        certificate::CertificateError::IoError { error, .. } => {
            CertError::IoError(error).cert_context(cert_path.clone())
        }
        from => CertError::CertificateError(from),
    })?;

    if pem.issuer()? == pem.subject()? {
        Ok(pem.subject_common_name()?)
    } else {
        Err(CertError::NotASelfSignedCertificate {
            path: cert_path.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            csr_path: None,
            bridge_location: BridgeLocation::Mosquitto,
        };

        assert_matches!(
            cmd.create_test_certificate(&NewCertificateConfig::default()),
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
            csr_path: None,
            bridge_location: BridgeLocation::Mosquitto,
        };

        assert!(cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .ok()
            .is_none());

        assert_eq!(fs::read(&cert_path).unwrap(), cert_content.as_bytes());
        assert_eq!(fs::read(&key_path).unwrap(), key_content.as_bytes());
    }

    #[test]
    fn create_certificate_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let cert_path = Utf8PathBuf::from("/non/existent/cert/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            csr_path: None,
            bridge_location: BridgeLocation::Mosquitto,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .unwrap_err();
        assert_matches!(cert_error, CertError::CertPathError { .. });
    }

    #[test]
    fn create_key_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = Utf8PathBuf::from("/non/existent/key/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            csr_path: None,
            bridge_location: BridgeLocation::Mosquitto,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }

    #[test]
    fn renew_certificate_without_key() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = Utf8PathBuf::from("/non/existent/key/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            csr_path: None,
            bridge_location: BridgeLocation::Mosquitto,
        };

        let cert_error = cmd
            .renew_test_certificate(&NewCertificateConfig::default())
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyNotFound { .. });
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> Utf8PathBuf {
        dir.path().join(filename).try_into().unwrap()
    }

    fn parse_pem_file(path: impl AsRef<Path>) -> Result<pem::Pem, String> {
        let content = fs::read(path).map_err(|err| err.to_string())?;
        pem::parse(content).map_err(|err| err.to_string())
    }
}
