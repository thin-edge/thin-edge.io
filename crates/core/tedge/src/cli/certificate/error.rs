use camino::Utf8PathBuf;
use certificate::translate_rustls_error;
use reqwest::StatusCode;
use std::error::Error;
use tedge_config::ConfigSettingError;
use tedge_config::TEdgeConfigError;
use tedge_utils::file::FileError;
use tedge_utils::paths::PathsError;

#[derive(thiserror::Error, Debug)]
pub enum CertError {
    #[error(
        r#"A certificate already exists and would be overwritten.
        Existing file: "{path}"
        Run `tedge cert remove` first to generate a new certificate.
    "#
    )]
    CertificateAlreadyExists { path: Utf8PathBuf },

    #[error(
        r#"No certificate has been attached to that device.
        Missing file: {path:?}
        Run `tedge cert create` to generate a new certificate.
    "#
    )]
    CertificateNotFound { path: Utf8PathBuf },

    #[error("I/O error accessing the certificate: {path:?}")]
    CertificateIoError {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        r#"No private key has been attached to that device.
        Missing file: {path:?}
        Run `tedge cert create` to generate a new key and certificate.
    "#
    )]
    KeyNotFound { path: Utf8PathBuf },

    #[error(
        r#"A private key already exists and would be overwritten.
        Existing file: {path:?}
        Run `tedge cert remove` first to generate a new certificate and private key.
    "#
    )]
    KeyAlreadyExists { path: Utf8PathBuf },

    #[error("I/O error accessing the private key: {path:?}")]
    KeyIoError {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    ConfigError(#[from] crate::ConfigError),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Invalid device.cert_path path: {0}")]
    CertPathError(PathsError),

    #[error("Invalid device.key_path path: {0}")]
    KeyPathError(PathsError),

    #[error("Invalid device.csr_path path: {0}")]
    CsrPathError(PathsError),

    #[error(transparent)]
    CertificateError(#[from] certificate::CertificateError),

    #[error(transparent)]
    PathsError(#[from] PathsError),

    #[error("Connection error: {0}")]
    ReqwestConnect(String),

    #[error("Request time out")]
    ReqwestTimeout,

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("Request returned with code: {0}")]
    StatusCode(StatusCode),

    #[error(transparent)]
    UrlParseError(#[from] url::ParseError),

    #[error(transparent)]
    TedgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    TedgeConfigSettingError(#[from] ConfigSettingError),

    #[error(transparent)]
    FileError(#[from] FileError),

    #[error(transparent)]
    IllFormedPk7Cert(#[from] crate::cli::certificate::c8y::IllFormedPk7Cert),

    #[error("Root certificate path {0} does not exist")]
    RootCertificatePathDoesNotExist(String),
}

impl CertError {
    /// Improve the error message in case the error in a IO error on the certificate file.
    pub fn cert_context(self, path: Utf8PathBuf) -> CertError {
        match self {
            CertError::IoError(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => CertError::CertificateAlreadyExists { path },
                std::io::ErrorKind::NotFound => CertError::CertificateNotFound { path },
                _ => CertError::CertificateIoError { path, source: err },
            },
            _ => self,
        }
    }

    /// Improve the error message in case the error in a IO error on the private key file.
    pub fn key_context(self, path: Utf8PathBuf) -> CertError {
        match self {
            CertError::IoError(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => CertError::KeyAlreadyExists { path },
                std::io::ErrorKind::NotFound => CertError::KeyNotFound { path },
                _ => CertError::KeyIoError { path, source: err },
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
pub fn get_webpki_error_from_reqwest(err: reqwest::Error) -> CertError {
    if let Some(tls_error) = err
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
        .and_then(|err| translate_rustls_error(err))
    {
        CertError::CertificateError(tls_error)
    } else {
        // any other Error type than `hyper::Error`
        if err.is_connect() {
            match err.source().and_then(|err| err.source()) {
                Some(io_error) => CertError::ReqwestConnect(format!("{io_error}")),
                None => CertError::ReqwestError(err),
            }
        } else if err.is_timeout() {
            CertError::ReqwestTimeout
        } else {
            CertError::ReqwestError(err)
        }
    }
}
