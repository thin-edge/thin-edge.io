use reqwest::StatusCode;
use std::error::Error;
use tedge_config::FilePath;
use tedge_config::{ConfigSettingError, TEdgeConfigError};
use tedge_users::UserSwitchError;
use tedge_utils::paths::PathsError;

#[derive(thiserror::Error, Debug)]
pub enum CertError {
    #[error(
        r#"A certificate already exists and would be overwritten.
        Existing file: "{path}"
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

    #[error(transparent)]
    UserSwitchError(#[from] UserSwitchError),

    #[error("HTTP Connection Problem: {msg} \nHint: {hint}")]
    CertificateValidationFailure { hint: String, msg: String },

    #[error(transparent)]
    TedgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    TedgeConfigSettingError(#[from] ConfigSettingError),
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

// Our source of error here is quite deep into the dependencies and we need to dig through that to get to our certificates validator errors which are Box<&dyn Error> through 3-4 levels
// source: hyper::Error(
//     Connect,
//     Custom {
//         kind: Other,
//         error: Custom {
//             kind: InvalidData,
//             error: InvalidCertificateData(
//                 ..., // This is where we need to get
//             ),
//         },
//     },
// )
// At the last layer we have the InvalidCertificateData error which is a Box<&dyn Error> derived from WebpkiError not included anymore, just as a String
// This chain may break if underlying crates change.
pub(crate) fn get_webpki_error_from_reqwest(err: reqwest::Error) -> CertError {
    if let Some(rustls::Error::InvalidCertificateData(inner)) = err
        // get `hyper::Error::Connect`
        .source()
        .and_then(|err| err.source())
        // From here the errors are converted from std::io::Error.
        // `Custom` type is `std::io::Error`; this is our first `Custom`.
        .and_then(|custom_error| custom_error.downcast_ref::<std::io::Error>())
        .and_then(|custom_error| custom_error.get_ref())
        // This is our second `Custom`.
        .and_then(|custom_error2| custom_error2.downcast_ref::<std::io::Error>())
        .and_then(|custom_error2| custom_error2.get_ref())
        // Get final error type from `rustls::Error`.
        .and_then(|rustls_error| rustls_error.downcast_ref::<rustls::Error>())
    {
        match inner {
            msg if msg.contains("CaUsedAsEndEntity") => CertError::CertificateValidationFailure {
                hint: "A CA certificate is used as an end-entity server certificate. Make sure that the certificate used is an end-entity certificate signed by CA certificate.".into(),
                msg: msg.to_string(),
            },

            msg if msg.contains("CertExpired") => CertError::CertificateValidationFailure {
                hint: "The server certificate has expired, the time it is being validated for is later than the certificate's `notAfter` time.".into(),
                msg: msg.to_string(),
            },

            msg if msg.contains("CertNotValidYet") => CertError::CertificateValidationFailure {
                hint: "The server certificate is not valid yet, the time it is being validated for is earlier than the certificate's `notBefore` time.".into(),
                msg: msg.to_string(),
            },

            msg if msg.contains("EndEntityUsedAsCa") => CertError::CertificateValidationFailure {
                hint: "An end-entity certificate is used as a server CA certificate. Make sure that the certificate used is signed by a correct CA certificate.".into(),
                msg: msg.to_string(),
            },

            msg if msg.contains("InvalidCertValidity") => CertError::CertificateValidationFailure {
                hint: "The server certificate validity period (`notBefore`, `notAfter`) is invalid, maybe the `notAfter` time is earlier than the `notBefore` time.".into(),
                msg: msg.to_string(),
            },

            _ => CertError::CertificateValidationFailure {
                hint: "Server certificate validation error.".into(),
                msg: inner.to_string(),
            },
        }
    } else {
        CertError::ReqwestError(err) // any other Error type than `hyper::Error`
    }
}
