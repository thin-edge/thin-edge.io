use reqwest::StatusCode;
use tedge_config::FilePath;

use crate::utils::paths::PathsError;

#[derive(thiserror::Error, Debug)]
pub enum CertError {
    #[error(
        r#"A certificate already exists and would be overwritten.
        Existing file: {path:?}
        Run `tedge cert remove` first to generate a new certificate.
    "#
    )]
    CertificateAlreadyExists { path: FilePath },

    #[error(
        r#"No certificate has been attached to that device.
        Missing file: {path:?}
        Run `tedge cert create` to generate a new certificate.
    "#
    )]
    CertificateNotFound { path: FilePath },

    #[error(
        r#"No private key has been attached to that device.
        Missing file: {path:?}
        Run `tedge cert create` to generate a new key and certificate.
    "#
    )]
    KeyNotFound { path: FilePath },

    #[error(
        r#"A private key already exists and would be overwritten.
        Existing file: {path:?}
        Run `tedge cert remove` first to generate a new certificate and private key.
    "#
    )]
    KeyAlreadyExists { path: FilePath },

    #[error(transparent)]
    ConfigError(#[from] crate::ConfigError),

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
}

impl CertError {
    /// Improve the error message in case the error in a IO error on the certificate file.
    pub fn cert_context(self, path: FilePath) -> CertError {
        match self {
            CertError::IoError(ref err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => CertError::CertificateAlreadyExists { path },
                std::io::ErrorKind::NotFound => CertError::CertificateNotFound { path },
                _ => self,
            },
            _ => self,
        }
    }

    /// Improve the error message in case the error in a IO error on the private key file.
    pub fn key_context(self, path: FilePath) -> CertError {
        match self {
            CertError::IoError(ref err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => CertError::KeyAlreadyExists { path },
                std::io::ErrorKind::NotFound => CertError::KeyNotFound { path },
                _ => self,
            },
            _ => self,
        }
    }
}
