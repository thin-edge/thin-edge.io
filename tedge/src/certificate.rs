use crate::config::{
    ConfigError, TEdgeConfig, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID, DEVICE_KEY_PATH,
};
use crate::utils::users;
use crate::utils::users::UserManager;
use crate::utils::{paths, paths::PathsError};
use crate::{
    command::{BuildCommand, Command, ExecutionContext},
    utils,
};
use certificate::{KeyCertPair, NewCertificateConfig, PemCertificate};
use reqwest::{StatusCode, Url};
use std::{
    convert::TryFrom,
    fs::{File, OpenOptions},
    io::prelude::*,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
pub enum TEdgeCertOpt {
    /// Create a self-signed device certificate
    Create {
        /// The device identifier to be used as the common name for the certificate
        #[structopt(long = "device-id")]
        id: String,
    },

    /// Show the device certificate, if any
    Show,

    /// Remove the device certificate
    Remove,

    /// Upload root certificate
    Upload(UploadCertOpt),
}

/// Create a self-signed device certificate
pub struct CreateCertCmd {
    /// The device identifier
    id: String,

    /// The path where the device certificate will be stored
    cert_path: String,

    /// The path where the device private key will be stored
    key_path: String,
}

/// Show the device certificate, if any
pub struct ShowCertCmd {
    /// The path where the device certificate will be stored
    cert_path: String,
}

/// Remove the device certificate
pub struct RemoveCertCmd {
    /// The path of the certificate to be removed
    cert_path: String,

    /// The path of the private key to be removed
    key_path: String,
}

#[derive(StructOpt, Debug)]
pub enum UploadCertOpt {
    /// Upload root certificate to Cumulocity
    ///
    /// The command will upload root certificate to Cumulocity.
    C8y {
        #[structopt(long = "user")]
        /// Provided username should be a Cumulocity user with tenant management permissions.
        /// The password is requested on /dev/tty, unless the $C8YPASS env var is set to the user password.
        username: String,
    },
}

impl BuildCommand for UploadCertOpt {
    fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        match self {
            UploadCertOpt::C8y { username } => {
                let device_id = config.device.id.ok_or_else(|| ConfigError::ConfigNotSet {
                    key: String::from(DEVICE_ID),
                })?;

                let path = PathBuf::try_from(config.device.cert_path.ok_or_else(|| {
                    ConfigError::ConfigNotSet {
                        key: String::from(DEVICE_CERT_PATH),
                    }
                })?)
                .expect("Path conversion failed unexpectedly!"); // This is Infallible that means it should never happen.

                let host = utils::config::parse_user_provided_address(config.c8y.url.ok_or_else(
                    || ConfigError::ConfigNotSet {
                        key: String::from(C8Y_URL),
                    },
                )?)?;

                Ok((UploadCertCmd {
                    device_id,
                    path,
                    host,
                    username,
                })
                .into_boxed())
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct CumulocityResponse {
    name: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadCertBody {
    name: String,
    cert_in_pem_format: String,
    auto_registration_enabled: bool,
    status: String,
}

struct UploadCertCmd {
    device_id: String,
    path: PathBuf,
    host: String,
    username: String,
}

impl Command for UploadCertCmd {
    fn description(&self) -> String {
        "upload root certificate".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        Ok(self.upload_certificate()?)
    }
}

impl UploadCertCmd {
    fn upload_certificate(&self) -> Result<(), CertError> {
        let client = reqwest::blocking::Client::new();

        // Read the password from /dev/tty
        // Unless a password is provided using the `C8YPASS` env var.
        let password = match std::env::var("C8YPASS") {
            Ok(password) => password,
            Err(_) => rpassword::read_password_from_tty(Some("Enter password: "))?,
        };

        // To post certificate c8y requires one of the following endpoints:
        // https://<tenant_id>.cumulocity.url.io/tenant/tenants/<tenant_id>/trusted-certificates
        // https://<tenant_domain>.cumulocity.url.io/tenant/tenants/<tenant_id>/trusted-certificates
        // and therefore we need to get tenant_id.
        let tenant_id = get_tenant_id_blocking(
            &client,
            build_get_tenant_id_url(&self.host)?,
            &self.username,
            &password,
        )?;
        Ok(self.post_certificate(&client, &tenant_id, &password)?)
    }

    fn post_certificate(
        &self,
        client: &reqwest::blocking::Client,
        tenant_id: &str,
        password: &str,
    ) -> Result<(), CertError> {
        let post_url = build_upload_certificate_url(&self.host, tenant_id)?;

        let post_body = UploadCertBody {
            auto_registration_enabled: true,
            cert_in_pem_format: read_cert_to_string(&self.path)?,
            name: self.device_id.clone(),
            status: "ENABLED".into(),
        };

        let res = client
            .post(post_url)
            .json(&post_body)
            .basic_auth(&self.username, Some(password))
            .send()?;

        match res.status() {
            StatusCode::OK | StatusCode::CREATED => {
                println!("Certificate uploaded successfully.");
                Ok(())
            }

            StatusCode::CONFLICT => {
                println!("Certificate already exists in the cloud.");
                Ok(())
            }

            status_code => {
                println!("Something went wrong: {}", status_code);
                Err(CertError::StatusCode(status_code))
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CertError {
    #[error(
        r#"A certificate already exists and would be overwritten.
        Existing file: {path:?}
        Run `tegde cert remove` first to generate a new certificate.
    "#
    )]
    CertificateAlreadyExists { path: String },

    #[error(
        r#"No certificate has been attached to that device.
        Missing file: {path:?}
        Run `tegde cert create` to generate a new certificate.
    "#
    )]
    CertificateNotFound { path: String },

    #[error(
        r#"No private key has been attached to that device.
        Missing file: {path:?}
        Run `tegde cert create` to generate a new key and certificate.
    "#
    )]
    KeyNotFound { path: String },

    #[error(
        r#"A private key already exists and would be overwritten.
        Existing file: {path:?}
        Run `tegde cert remove` first to generate a new certificate and private key.
    "#
    )]
    KeyAlreadyExists { path: String },

    #[error(transparent)]
    ConfigError(#[from] ConfigError),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error("Invalid device.cert.path path: {0}")]
    CertPathError(PathsError),

    #[error("Invalid device.key.path path: {0}")]
    KeyPathError(PathsError),

    #[error(transparent)]
    CertificateError(#[from] certificate::CertificateError),

    #[error(
        r#"Certificate read error at: {1:?}
        Run `tedge cert create` if you want to create a new certificate."#
    )]
    CertificateReadFailed(#[source] std::io::Error, String),

    #[error(transparent)]
    PathsError(#[from] PathsError),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("Request returned with code: {0}")]
    StatusCode(StatusCode),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    UserSwitchError(#[from] users::UserSwitchError),
}

impl CertError {
    /// Improve the error message in case the error in a IO error on the certificate file.
    fn cert_context(self, path: &str) -> CertError {
        match self {
            CertError::IoError(ref err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    CertError::CertificateAlreadyExists { path: path.into() }
                }
                std::io::ErrorKind::NotFound => {
                    CertError::CertificateNotFound { path: path.into() }
                }
                _ => self,
            },
            _ => self,
        }
    }

    /// Improve the error message in case the error in a IO error on the private key file.
    fn key_context(self, path: &str) -> CertError {
        match self {
            CertError::IoError(ref err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    CertError::KeyAlreadyExists { path: path.into() }
                }
                std::io::ErrorKind::NotFound => CertError::KeyNotFound { path: path.into() },
                _ => self,
            },
            _ => self,
        }
    }
}

impl BuildCommand for TEdgeCertOpt {
    fn build_command(self, config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        let cmd =
            match self {
                TEdgeCertOpt::Create { id } => {
                    let cmd = CreateCertCmd {
                        id,
                        cert_path: config.device.cert_path.ok_or_else(|| {
                            ConfigError::ConfigNotSet {
                                key: String::from(DEVICE_CERT_PATH),
                            }
                        })?,
                        key_path: config.device.key_path.ok_or_else(|| {
                            ConfigError::ConfigNotSet {
                                key: String::from(DEVICE_KEY_PATH),
                            }
                        })?,
                    };
                    cmd.into_boxed()
                }

                TEdgeCertOpt::Show => {
                    let cmd = ShowCertCmd {
                        cert_path: config.device.cert_path.ok_or_else(|| {
                            ConfigError::ConfigNotSet {
                                key: String::from(DEVICE_CERT_PATH),
                            }
                        })?,
                    };
                    cmd.into_boxed()
                }

                TEdgeCertOpt::Remove => {
                    let cmd = RemoveCertCmd {
                        cert_path: config.device.cert_path.ok_or_else(|| {
                            ConfigError::ConfigNotSet {
                                key: String::from(DEVICE_CERT_PATH),
                            }
                        })?,
                        key_path: config.device.key_path.ok_or_else(|| {
                            ConfigError::ConfigNotSet {
                                key: String::from(DEVICE_KEY_PATH),
                            }
                        })?,
                    };
                    cmd.into_boxed()
                }

                TEdgeCertOpt::Upload(cmd) => cmd.build_command(config)?,
            };

        Ok(cmd)
    }
}

impl Command for CreateCertCmd {
    fn description(&self) -> String {
        format!("create a test certificate for the device {}.", self.id)
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let config = NewCertificateConfig::default();
        let () = self.create_test_certificate(&config, &context.user_manager)?;
        let () = self.update_tedge_config()?;
        Ok(())
    }
}

impl Command for ShowCertCmd {
    fn description(&self) -> String {
        "show the device certificate".into()
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let () = self.show_certificate()?;
        Ok(())
    }
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self, context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let () = self.remove_certificate(&context.user_manager)?;
        let () = self.update_tedge_config()?;
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

        let cert_path = Path::new(&self.cert_path);
        let key_path = Path::new(&self.key_path);

        paths::validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
        paths::validate_parent_dir_exists(key_path).map_err(CertError::KeyPathError)?;

        // Creating files with permission 644
        let mut cert_file =
            create_new_file(&self.cert_path).map_err(|err| err.cert_context(&self.cert_path))?;
        let mut key_file =
            create_new_file(&self.key_path).map_err(|err| err.key_context(&self.key_path))?;

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

    fn update_tedge_config(&self) -> Result<(), CertError> {
        let mut config = TEdgeConfig::from_default_config()?;
        config.device.id = Some(self.id.clone());

        let _ = config.write_to_default_config()?;

        Ok(())
    }
}

impl ShowCertCmd {
    fn show_certificate(&self) -> Result<(), CertError> {
        let cert_path = &self.cert_path;
        let pem = PemCertificate::from_pem_file(cert_path).map_err(|err| match err {
            certificate::CertificateError::IoError(from) => {
                CertError::IoError(from).cert_context(cert_path)
            }
            from => CertError::CertificateError(from),
        })?;

        println!("Device certificate: {}", cert_path);
        println!("Subject: {}", pem.subject()?);
        println!("Issuer: {}", pem.issuer()?);
        println!("Valid from: {}", pem.not_before()?);
        println!("Valid up to: {}", pem.not_after()?);
        println!("Thumbprint: {}", pem.thumbprint()?);
        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self, user_manager: &UserManager) -> Result<(), CertError> {
        let _user_guard = user_manager.become_user(users::BROKER_USER)?;
        std::fs::remove_file(&self.cert_path).or_else(ok_if_not_found)?;
        std::fs::remove_file(&self.key_path).or_else(ok_if_not_found)?;

        Ok(())
    }

    fn update_tedge_config(&self) -> Result<(), CertError> {
        let mut config = TEdgeConfig::from_default_config()?;
        config.device.id = None;

        let _ = config.write_to_default_config()?;

        Ok(())
    }
}

fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

fn read_cert_to_string(path: impl AsRef<Path>) -> Result<String, CertError> {
    let path = path.as_ref();
    let path = utils::paths::pathbuf_to_string(path.to_owned())?;

    let mut file =
        std::fs::File::open(&path).map_err(|err| CertError::CertificateReadFailed(err, path))?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    Ok(content)
}

fn create_new_file(path: &str) -> Result<File, CertError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn get_tenant_id_blocking(
    client: &reqwest::blocking::Client,
    url: Url,
    username: &str,
    password: &str,
) -> Result<String, CertError> {
    let res = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()?
        .error_for_status()?;

    let body = res.json::<CumulocityResponse>()?;
    Ok(body.name)
}

fn build_get_tenant_id_url(host: &str) -> Result<Url, CertError> {
    let url = format!("https://{}/tenant/currentTenant", host);
    Ok(Url::parse(&url)?)
}

fn build_upload_certificate_url(host: &str, tenant_id: &str) -> Result<Url, CertError> {
    let url_str = format!(
        "https://{}/tenant/tenants/{}/trusted-certificates",
        host, tenant_id
    );

    Ok(Url::parse(&url_str)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::users::UserManager;
    use assert_matches::assert_matches;
    use std::fs::File;
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

        assert!(cmd
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
            .err()
            .is_none());
        assert_eq!(parse_pem_file(&cert_path).unwrap().tag, "CERTIFICATE");
        assert_eq!(parse_pem_file(&key_path).unwrap().tag, "PRIVATE KEY");
    }

    #[test]
    fn check_certificate_is_not_overwritten() {
        let cert_content = "some cert content";
        let key_content = "some key content";
        let cert_file = temp_file_with_content(cert_content);
        let key_file = temp_file_with_content(key_content);
        let id = "my-device-id";

        let cmd = CreateCertCmd {
            id: String::from(id),
            cert_path: String::from(cert_file.path().to_str().unwrap()),
            key_path: String::from(key_file.path().to_str().unwrap()),
        };

        assert!(cmd
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
            .ok()
            .is_none());

        let mut cert_file = cert_file.reopen().unwrap();
        assert_eq!(file_content(&mut cert_file), cert_content);

        let mut key_file = key_file.reopen().unwrap();
        assert_eq!(file_content(&mut key_file), key_content);
    }

    #[test]
    fn create_certificate_in_non_existent_directory() {
        let dir = tempdir().unwrap();
        let key_path = temp_file_path(&dir, "my-device-key.pem");

        let cmd = CreateCertCmd {
            id: "my-device-id".to_string(),
            cert_path: "/non/existent/cert/path".to_string(),
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

        let cmd = CreateCertCmd {
            id: "my-device-id".to_string(),
            cert_path,
            key_path: "/non/existent/key/path".to_string(),
        };

        let cert_error = cmd
            .create_test_certificate(&NewCertificateConfig::default(), &UserManager::new())
            .unwrap_err();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }

    #[test]
    fn build_get_tenant_id_url_should_return_error_given_invalid_host() {
        let result = build_get_tenant_id_url("%");

        assert!(result.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_should_return_error_given_wrong_credentials() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDpmYWlsZWR0ZXN0"; // Base64 encoded test:failedtest

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_returns_correct_response() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let expected = "test";

        let _serv = mockito::mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test").unwrap();

        assert_eq!(res, expected);
    }

    #[test]
    fn get_tenant_id_blocking_response_should_return_error_when_response_has_no_name_field() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"test":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/test/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn make_upload_certificate_url_correct_parameters_return_url() {
        let url = "test";
        let tenant_id = "test";
        let expected =
            reqwest::Url::parse("https://test/tenant/tenants/test/trusted-certificates").unwrap();

        let result = build_upload_certificate_url(url, tenant_id).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn make_upload_certificate_returns_error_given_incorrect_url() {
        let url = "@";
        let tenant_id = "test";

        let result = build_upload_certificate_url(url, tenant_id).unwrap_err();
        assert_matches!(result, CertError::UrlParseError(url::ParseError::EmptyHost));
    }

    fn temp_file_path(dir: &TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }

    fn file_content(file: &mut File) -> String {
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        content
    }

    fn parse_pem_file(path: &str) -> Result<pem::Pem, String> {
        let mut file = File::open(path).map_err(|err| err.to_string())?;
        let content = file_content(&mut file);

        pem::parse(content).map_err(|err| err.to_string())
    }
}
