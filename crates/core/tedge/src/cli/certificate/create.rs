use super::error::CertError;
use crate::cli::certificate::show::ShowCertCmd;
use crate::command::Command;
use crate::log::MaybeFancy;
use camino::Utf8PathBuf;
use certificate::KeyCertPair;
use certificate::KeyKind;
use certificate::NewCertificateConfig;
use certificate::PemCertificate;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tedge_utils::paths::validate_parent_dir_exists;
use tokio::fs::File;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

/// Create a self-signed device certificate
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
}

#[async_trait::async_trait]
impl Command for CreateCertCmd {
    fn description(&self) -> String {
        format!("create a test certificate for the device {}.", self.id)
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = NewCertificateConfig::default();
        self.create_test_certificate(&config).await?;
        eprintln!("Certificate was successfully created\n");
        let show_cert_cmd = ShowCertCmd {
            cert_path: self.cert_path.clone(),
        };
        show_cert_cmd.show_certificate().await?;
        Ok(())
    }
}

impl CreateCertCmd {
    pub async fn create_test_certificate(
        &self,
        config: &NewCertificateConfig,
    ) -> Result<(), CertError> {
        let cert = KeyCertPair::new_selfsigned_certificate(config, &self.id, &KeyKind::New)?;

        let cert_path = &self.cert_path;
        persist_new_public_key(
            cert_path,
            cert.certificate_pem_string()?,
            &self.user,
            &self.group,
        )
        .await
        .map_err(|err| err.cert_context(cert_path.clone()))?;

        let key_path = &self.key_path;
        persist_new_private_key(
            key_path,
            cert.private_key_pem_string()?,
            &self.user,
            &self.group,
        )
        .await
        .map_err(|err| err.key_context(key_path.clone()))?;
        Ok(())
    }
}

pub async fn persist_new_public_key(
    cert_path: &Utf8PathBuf,
    pem_string: String,
    user: &str,
    group: &str,
) -> Result<(), CertError> {
    validate_parent_dir_exists(cert_path)
        .await
        .map_err(CertError::CertPathError)?;
    let key_file = create_new_file(cert_path, user, group).await?;
    persist_public_key(key_file, pem_string).await?;
    Ok(())
}

pub async fn persist_new_private_key(
    key_path: &Utf8PathBuf,
    key: certificate::Zeroizing<String>,
    user: &str,
    group: &str,
) -> Result<(), CertError> {
    validate_parent_dir_exists(key_path)
        .await
        .map_err(CertError::KeyPathError)?;
    let key_file = create_new_file(key_path, user, group).await?;
    persist_private_key(key_file, key).await?;
    Ok(())
}

pub async fn override_public_key(
    cert_path: &Utf8PathBuf,
    pem_string: String,
) -> Result<(), CertError> {
    validate_parent_dir_exists(cert_path)
        .await
        .map_err(CertError::CertPathError)?;
    let key_file = override_file(cert_path).await?;
    persist_public_key(key_file, pem_string).await?;
    Ok(())
}

async fn create_new_file(
    path: &Utf8PathBuf,
    user: &str,
    group: &str,
) -> Result<File, std::io::Error> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;

    // Ignore errors - This was the behavior with the now deprecated user manager.
    // - When `tedge cert create` is not run as root, a certificate is created but owned by the user running the command.
    // - A better approach could be to remove this `chown` and run the command as mosquitto.
    let _ =
        tedge_utils::file::change_user_and_group(path.into(), user.to_string(), group.to_string())
            .await;

    Ok(file)
}

// Allow permissions_set_readonly_false as the file will be make readonly once its content updated
#[allow(clippy::permissions_set_readonly_false)]
async fn override_file(path: impl AsRef<Path>) -> Result<File, std::io::Error> {
    let path = path.as_ref();

    // If the file already exists, make sure it can be overwritten.
    // However, defer any error to the open step, to give better context to the user
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        let mut perm = metadata.permissions();
        perm.set_readonly(false);
        let _ = tokio::fs::set_permissions(path, perm).await;
    };

    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .await
}

pub async fn reuse_private_key(key_path: &Utf8PathBuf) -> Result<KeyKind, std::io::Error> {
    tokio::fs::read_to_string(key_path)
        .await
        .map(|keypair_pem| KeyKind::Reuse { keypair_pem })
}

async fn persist_private_key(
    mut key_file: File,
    cert_key: certificate::Zeroizing<String>, // Zero the private key on drop
) -> Result<(), std::io::Error> {
    // Make sure the key is secret, before write
    File::set_permissions(&key_file, Permissions::from_mode(0o600)).await?;
    key_file.write_all(cert_key.as_bytes()).await?;
    key_file.sync_all().await?;

    // Prevent the key to be overwritten
    File::set_permissions(&key_file, Permissions::from_mode(0o400)).await?;
    Ok(())
}

async fn persist_public_key(mut key_file: File, cert_pem: String) -> Result<(), std::io::Error> {
    key_file.write_all(cert_pem.as_bytes()).await?;
    key_file.sync_all().await?;

    // Make the file public
    File::set_permissions(&key_file, Permissions::from_mode(0o444)).await?;
    Ok(())
}

pub async fn cn_of_self_signed_certificate(cert_path: &Utf8PathBuf) -> Result<String, CertError> {
    let cert = tokio::fs::read_to_string(cert_path)
        .await
        .map_err(|err| CertError::IoError(err).cert_context(cert_path.clone()))?;
    let pem = PemCertificate::from_pem_string(&cert)?;

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
    use crate::cli::certificate::test_helpers::*;
    use assert_matches::assert_matches;
    use std::fs;
    use tempfile::*;

    #[tokio::test]
    async fn basic_usage() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let id = "my-device-id";

        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: cert_path.clone(),
            key_path: key_path.clone(),
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
        };

        assert_matches!(
            cmd.create_test_certificate(&NewCertificateConfig::default())
                .await,
            Ok(())
        );
        assert_eq!(parse_pem_file(&cert_path).tag, "CERTIFICATE");
        assert_eq!(parse_pem_file(&key_path).tag, "PRIVATE KEY");
    }

    #[tokio::test]
    async fn check_certificate_is_not_overwritten() {
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
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
        };

        assert!(cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .await
            .ok()
            .is_none());

        assert_eq!(fs::read(&cert_path).unwrap(), cert_content.as_bytes());
        assert_eq!(fs::read(&key_path).unwrap(), key_content.as_bytes());
    }

    #[tokio::test]
    async fn create_certificate_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let key_path = temp_file_path(&dir, "my-device-key.pem");
        let cert_path = Utf8PathBuf::from("/non/existent/cert/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .await
            .unwrap_err();
        assert_matches!(cert_error, CertError::CertPathError { .. });
    }

    #[tokio::test]
    async fn create_key_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let cert_path = temp_file_path(&dir, "my-device-cert.pem");
        let key_path = Utf8PathBuf::from("/non/existent/key/path");

        let cmd = CreateCertCmd {
            id: "my-device-id".into(),
            cert_path,
            key_path,
            user: "mosquitto".to_string(),
            group: "mosquitto".to_string(),
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default())
            .await
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }
}
