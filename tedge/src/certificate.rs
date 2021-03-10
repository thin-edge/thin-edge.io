use crate::command::{BuildCommand, Command};
use crate::config::{
    ConfigError, TEdgeConfig, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID, DEVICE_KEY_PATH,
};
use crate::utils::{paths, paths::PathsError};
use chrono::offset::Utc;
use chrono::Duration;
use rcgen::Certificate;
use rcgen::CertificateParams;
use rcgen::RcgenError;
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
                .expect("Path conversion failed unexpectedly!"); // This is Infallible that means it can never happen.

                let url = config.c8y.url.ok_or_else(|| ConfigError::ConfigNotSet {
                    key: String::from(C8Y_URL),
                })?;

                Ok((UploadCertCmd {
                    device_id,
                    path,
                    url,
                    username,
                })
                .into_boxed())
            }
        }
    }
}

#[derive(serde::Deserialize, Debug)]
struct CumulocityResponse {
    name: String,
}

#[derive(serde::Serialize, Debug)]
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
    url: String,
    username: String,
}

impl Command for UploadCertCmd {
    fn description(&self) -> String {
        "upload root certificate".into()
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.upload_certificate()?)
    }
}

impl UploadCertCmd {
    fn upload_certificate(&self) -> Result<(), CertError> {
        let client = reqwest::blocking::Client::new();

        let password = rpassword::read_password_from_tty(Some("Enter password: \n"))?;

        // To post certificate c8y requires one of the following endpoints:
        // https://<tenant_id>.cumulocity.url.io/tenant/tenants/<tenant_id>/trusted-certificates
        // https://<tenant_domain>.cumulocity.url.io/tenant/tenants/<tenant_id>/trusted-certificates
        // and therefore we need to get tenant_id.
        let tenant_id = get_tenant_id_blocking(&client, &self.url, &self.username, &password)?;
        Ok(self.post_certificate(&client, &tenant_id, &password)?)
    }

    fn post_certificate(
        &self,
        client: &reqwest::blocking::Client,
        tenant_id: &str,
        password: &str,
    ) -> Result<(), CertError> {
        let post_url = make_upload_certificate_url(&self.url, tenant_id)?;

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
    #[error(r#"The string '{name:?}' contains characters which cannot be used in a name"#)]
    InvalidCharacter { name: String },

    #[error(r#"The empty string cannot be used as a name"#)]
    EmptyName,

    #[error(
        r#"The string '{name:?}' is more than {} characters long and cannot be used as a name"#,
        MAX_CN_SIZE
    )]
    TooLongName { name: String },

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

    #[error("Cryptography related error")]
    CryptographyError(#[from] RcgenError),

    #[error("PEM file format error")]
    PemError(#[from] x509_parser::error::PEMError),

    #[error("X509 file format error: {0}")]
    X509Error(String), // One cannot use x509_parser::error::X509Error unless one use `nom`.

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("Request returned with code: {0}")]
    StatusCode(StatusCode),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),
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

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        let config = CertConfig::default();
        let () = self.create_test_certificate(&config)?;
        let () = self.update_tedge_config()?;
        Ok(())
    }
}
impl Command for ShowCertCmd {
    fn description(&self) -> String {
        "show the device certificate".into()
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        let () = self.show_certificate()?;
        Ok(())
    }
}

impl Command for RemoveCertCmd {
    fn description(&self) -> String {
        "remove the device certificate".into()
    }

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        let () = self.remove_certificate()?;
        let () = self.update_tedge_config()?;
        Ok(())
    }
}

struct CertConfig {
    test_cert: TestCertConfig,
}

struct TestCertConfig {
    validity_period_days: u32,
    organization_name: String,
    organizational_unit_name: String,
}

impl Default for CertConfig {
    fn default() -> Self {
        CertConfig {
            test_cert: TestCertConfig::default(),
        }
    }
}

impl Default for TestCertConfig {
    fn default() -> Self {
        TestCertConfig {
            validity_period_days: 365,
            organization_name: "Thin Edge".into(),
            organizational_unit_name: "Test Device".into(),
        }
    }
}

impl CreateCertCmd {
    fn create_test_certificate(&self, config: &CertConfig) -> Result<(), CertError> {
        check_identifier(&self.id)?;

        let cert_path = Path::new(&self.cert_path);
        let key_path = Path::new(&self.key_path);

        paths::validate_parent_dir_exists(cert_path).map_err(CertError::CertPathError)?;
        paths::validate_parent_dir_exists(key_path).map_err(CertError::KeyPathError)?;

        // Creating files with permission 644
        let mut cert_file =
            create_new_file(&self.cert_path).map_err(|err| err.cert_context(&self.cert_path))?;
        let mut key_file =
            create_new_file(&self.key_path).map_err(|err| err.key_context(&self.key_path))?;

        let cert = new_selfsigned_certificate(&config, &self.id)?;

        let cert_pem = cert.serialize_pem()?;
        cert_file.write_all(cert_pem.as_bytes())?;
        cert_file.sync_all()?;

        // Prevent the certificate to be overwritten
        paths::set_permission(&cert_file, 0o444)?;

        {
            // Make sure the key is secret, before write
            paths::set_permission(&key_file, 0o600)?;

            // Zero the private key on drop
            let cert_key = zeroize::Zeroizing::new(cert.serialize_private_key_pem());
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
        config.device.cert_path = Some(self.cert_path.clone());
        config.device.key_path = Some(self.key_path.clone());

        let _ = config.write_to_default_config()?;

        Ok(())
    }
}

impl ShowCertCmd {
    fn show_certificate(&self) -> Result<(), CertError> {
        let cert_path = &self.cert_path;
        let pem = read_pem(cert_path).map_err(|err| err.cert_context(cert_path))?;
        let x509 = extract_certificate(&pem)?;
        let tbs_certificate = x509.tbs_certificate;

        println!("Device certificate: {}", cert_path);
        println!("Subject: {}", tbs_certificate.subject.to_string());
        println!("Issuer: {}", tbs_certificate.issuer.to_string());
        println!(
            "Valid from: {}",
            tbs_certificate.validity.not_before.to_rfc2822()
        );
        println!(
            "Valid up to: {}",
            tbs_certificate.validity.not_after.to_rfc2822()
        );

        Ok(())
    }
}

impl RemoveCertCmd {
    fn remove_certificate(&self) -> Result<(), CertError> {
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

const MAX_CN_SIZE: usize = 64;

fn check_identifier(id: &str) -> Result<(), CertError> {
    if id.is_empty() {
        return Err(CertError::EmptyName);
    } else if id.len() > MAX_CN_SIZE {
        return Err(CertError::TooLongName { name: id.into() });
    } else if id.contains(char::is_control) {
        return Err(CertError::InvalidCharacter { name: id.into() });
    }

    Ok(())
}

fn extract_certificate(
    pem: &x509_parser::pem::Pem,
) -> Result<x509_parser::certificate::X509Certificate, CertError> {
    let x509 = pem.parse_x509().map_err(|err| {
        // The x509 error is wrapped into a `nom::Err`
        // and cannot be extracted without pattern matching on that type
        // So one simply extract the error as a string,
        // to avoid a dependency on the `nom` crate.
        let x509_error_string = format!("{}", err);
        CertError::X509Error(x509_error_string)
    })?;
    Ok(x509)
}

fn read_pem(path: &str) -> Result<x509_parser::pem::Pem, CertError> {
    let file = std::fs::File::open(path)?;
    let (pem, _) = x509_parser::pem::Pem::read(std::io::BufReader::new(file))?;
    Ok(pem)
}

fn read_cert_to_string(path: impl AsRef<Path>) -> Result<String, CertError> {
    let mut file = std::fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    Ok(content)
}

fn create_new_file(path: &str) -> Result<File, CertError> {
    Ok(OpenOptions::new().write(true).create_new(true).open(path)?)
}

fn new_selfsigned_certificate(config: &CertConfig, id: &str) -> Result<Certificate, RcgenError> {
    let mut distinguished_name = rcgen::DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, id);
    distinguished_name.push(
        rcgen::DnType::OrganizationName,
        &config.test_cert.organization_name,
    );
    distinguished_name.push(
        rcgen::DnType::OrganizationalUnitName,
        &config.test_cert.organizational_unit_name,
    );

    let today = Utc::now();
    let not_before = today - Duration::days(1); // Ensure the certificate is valid today
    let not_after = today + Duration::days(config.test_cert.validity_period_days.into());

    let mut params = CertificateParams::default();
    params.distinguished_name = distinguished_name;
    params.not_before = not_before;
    params.not_after = not_after;
    params.alg = &rcgen::PKCS_ECDSA_P256_SHA256; // ECDSA signing using the P-256 curves and SHA-256 hashing as per RFC 5758
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained); // IsCa::SelfSignedOnly is rejected by C8Y

    Certificate::from_params(params)
}

fn get_tenant_id_blocking(
    client: &reqwest::blocking::Client,
    url: &str,
    username: &str,
    password: &str,
) -> Result<String, CertError> {
    #[cfg(not(test))]
    let query_url = format!("https://{}/tenant/currentTenant", url);
    #[cfg(test)]
    let query_url = format!("http://{}/tenant/currentTenant", url);

    let parsed_url = Url::parse(&query_url)?;

    let res = client
        .get(parsed_url)
        .basic_auth(username, Some(password))
        .send()?
        .error_for_status()?;

    let body = res.json::<CumulocityResponse>()?;
    Ok(body.name)
}

fn make_upload_certificate_url(url: &str, tenant_id: &str) -> Result<Url, CertError> {
    let url_str = format!(
        "https://{}/tenant/tenants/{}/trusted-certificates",
        url, tenant_id
    );

    Ok(Url::parse(&url_str)?)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let verbose = 0;

        assert!(cmd.execute(verbose).err().is_none());
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
        let verbose = 0;

        assert!(cmd.execute(verbose).ok().is_none());

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
        let verbose = 0;

        let error = cmd.execute(verbose).unwrap_err();
        let cert_error = error.downcast_ref::<CertError>().unwrap();
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
        let verbose = 0;

        let error = cmd.execute(verbose).unwrap_err();
        let cert_error = error.downcast_ref::<CertError>().unwrap();
        assert_matches!(cert_error, CertError::KeyPathError { .. });
    }

    #[test]
    fn get_tenant_id_blocking_should_return_error_given_malformed_url() {
        let client = reqwest::blocking::Client::new();

        let _serv = mockito::mock("GET", "/test/tenant/currentTenant").create();

        let res = get_tenant_id_blocking(&client, "%", "", "");
        assert!(res.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_should_return_error_given_wrong_credentials() {
        let client = reqwest::blocking::Client::new();

        let request_url = mockito::server_address().to_string();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDpmYWlsZWR0ZXN0"; // Base64 encoded test:failedtest

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, &request_url, "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_returns_correct_response() {
        let client = reqwest::blocking::Client::new();

        let request_url = format!("{}", mockito::server_address());

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

        let res = get_tenant_id_blocking(&client, &request_url, "test", "test").unwrap();

        assert_eq!(res, expected);
    }

    #[test]
    fn get_tenant_id_blocking_response_should_return_error_when_response_has_no_name_field() {
        let client = reqwest::blocking::Client::new();

        // let request_url = format!("{}", mockito::server_url());

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"test":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/test/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, &mockito::server_url(), "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn make_upload_certificate_url_correct_parameters_return_url() {
        let url = "test";
        let tenant_id = "test";
        let expected =
            reqwest::Url::parse("https://test/tenant/tenants/test/trusted-certificates").unwrap();

        let result = make_upload_certificate_url(url, tenant_id).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn make_upload_certificate_returns_error_given_incorrect_url() {
        let url = "@";
        let tenant_id = "test";
        let _expected = CertError::UrlParseError(url::ParseError::EmptyHost);

        let result = make_upload_certificate_url(url, tenant_id).unwrap_err();
        assert_matches!(result, _expected);
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
