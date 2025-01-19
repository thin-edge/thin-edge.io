use super::error::CertError;
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
use tedge_config::TEdgeConfigLocation;
use tedge_config::WritableKey;
use tedge_utils::paths::set_permission;
use tedge_utils::paths::validate_parent_dir_exists;

/// Create self-signed device certificate
pub struct CreateCertCmd {
    /// The device identifier
    pub id: String,

    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,

    /// The path where the device private key will be stored
    pub key_path: Utf8PathBuf,

    /// The owner of the private key
    pub user: String,
    pub group: String,

    /// The configs required to update the tedge.toml file
    pub config_location: TEdgeConfigLocation,
    pub writable_key: WritableKey,
}

impl Command for CreateCertCmd {
    fn description(&self) -> String {
        format!("create a test certificate for the device {}.", self.id)
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = NewCertificateConfig::default();
        self.create_test_certificate(&config)?;
        eprintln!("Certificate was successfully created");
        set_device_id(&self.config_location, &self.writable_key, &self.id)?;
        eprintln!("'{}' is set to {}", self.writable_key, self.id);
        Ok(())
    }
}

impl CreateCertCmd {
    pub fn create_test_certificate(&self, config: &NewCertificateConfig) -> Result<(), CertError> {
        let cert = KeyCertPair::new_selfsigned_certificate(config, &self.id, &KeyKind::New)?;

        let cert_path = &self.cert_path;
        persist_new_public_key(
            cert_path,
            cert.certificate_pem_string()?,
            &self.user,
            &self.group,
        )
        .map_err(|err| err.cert_context(cert_path.clone()))?;

        let key_path = &self.key_path;
        persist_new_private_key(
            key_path,
            cert.private_key_pem_string()?,
            &self.user,
            &self.group,
        )
        .map_err(|err| err.key_context(key_path.clone()))?;
        Ok(())
    }
}

pub fn persist_new_public_key(
    cert_path: &Utf8PathBuf,
    pem_string: String,
    user: &str,
    group: &str,
) -> Result<(), CertError> {
    validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
    persist_public_key(create_new_file(cert_path, user, group)?, pem_string)?;
    Ok(())
}

pub fn persist_new_private_key(
    key_path: &Utf8PathBuf,
    key: certificate::Zeroizing<String>,
    user: &str,
    group: &str,
) -> Result<(), CertError> {
    validate_parent_dir_exists(key_path).map_err(CertError::KeyPathError)?;
    persist_private_key(create_new_file(key_path, user, group)?, key)?;
    Ok(())
}

pub fn override_public_key(cert_path: &Utf8PathBuf, pem_string: String) -> Result<(), CertError> {
    validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
    persist_public_key(override_file(cert_path)?, pem_string)?;
    Ok(())
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

pub fn reuse_private_key(key_path: &Utf8PathBuf) -> Result<KeyKind, std::io::Error> {
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

pub(crate) fn set_device_id(
    config_location: &TEdgeConfigLocation,
    writable_key: &WritableKey,
    id: &str,
) -> Result<(), anyhow::Error> {
    config_location
        .update_toml(&|dto, _reader| dto.try_update_str(writable_key, id).map_err(|e| e.into()))
        .map_err(anyhow::Error::new)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::fs;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn basic_usage() {
        let ttd = TempTedgeDir::new();
        let cert_path = ttd.utf8_path().join("my-device-cert.pem");
        let key_path = ttd.utf8_path().join("my-device-key.pem");
        let id = "my-device-id";

        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            config_location: TEdgeConfigLocation::from_custom_root(ttd.path()),
            writable_key: WritableKey::DeviceId,
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
        let ttd = TempTedgeDir::new();
        let cert_path = ttd.utf8_path().join("my-device-cert.pem");
        let key_path = ttd.utf8_path().join("my-device-key.pem");

        let cert_content = "some cert content";
        let key_content = "some key content";

        fs::write(&cert_path, cert_content).unwrap();
        fs::write(&key_path, key_content).unwrap();

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            config_location: TEdgeConfigLocation::from_custom_root(ttd.path()),
            writable_key: WritableKey::DeviceId,
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
        let ttd = TempTedgeDir::new();
        let cert_path = ttd.utf8_path().join("/non/existent/cert/path");
        let key_path = ttd.utf8_path().join("my-device-key.pem");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            config_location: TEdgeConfigLocation::from_custom_root(ttd.path()),
            writable_key: WritableKey::DeviceId,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .unwrap_err();
        assert_matches!(cert_error, CertError::CertPathError { .. });
    }

    #[test]
    fn create_key_in_non_existent_directory() {
        let ttd = TempTedgeDir::new();
        let cert_path = ttd.utf8_path().join("my-device-cert.pem");
        let key_path = ttd.utf8_path().join("/non/existent/key/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
            config_location: TEdgeConfigLocation::from_custom_root(ttd.path()),
            writable_key: WritableKey::DeviceId,
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }

    #[test]
    fn write_device_id() {
        let ttd = TempTedgeDir::new();
        let id = "my-device-id";
        let config_location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let writable_key = WritableKey::DeviceId;

        set_device_id(&config_location, &writable_key, id).unwrap();

        let toml_path = ttd.utf8_path().join("tedge.toml");
        let toml = fs::read_to_string(&toml_path)
            .unwrap()
            .parse::<toml::Value>()
            .unwrap();
        assert_eq!(toml["device"]["id"].as_str(), Some(id));
    }

    #[test]
    fn write_device_id_with_cloud_profile() {
        let ttd = TempTedgeDir::new();
        let id = "my-device-id";
        let config_location = TEdgeConfigLocation::from_custom_root(ttd.path());
        let writable_key = WritableKey::C8yDeviceId(Some("second".to_string()));

        set_device_id(&config_location, &writable_key, id).unwrap();

        let toml_path = ttd.utf8_path().join("tedge.toml");
        let toml = fs::read_to_string(&toml_path)
            .unwrap()
            .parse::<toml::Value>()
            .unwrap();
        assert_eq!(
            toml["c8y"]["profiles"]["second"]["device"]["id"].as_str(),
            Some(id)
        );
    }

    fn parse_pem_file(path: impl AsRef<Path>) -> Result<pem::Pem, String> {
        let content = fs::read(path).map_err(|err| err.to_string())?;
        pem::parse(content).map_err(|err| err.to_string())
    }
}
