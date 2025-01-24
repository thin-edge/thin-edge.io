use std::path::PathBuf;

use device_id::DeviceIdError;
pub use zeroize::Zeroizing;

pub mod device_id;

// TODO: remove/reduce the rustls-0.21/0.22 split
//
// a split between rustls-0.22 and rustls-0.21 compatible versions of public items of the crate was introduced as a
// result of upgrading to rustls-0.22
//
// this split should be removed/reduced by fixing dependencies/erasing types

// only rumqttc uses rustls 0.22, so expose 0.21 at top level to minimize changes
pub mod rustls021;
pub use rustls021::*;

pub mod rustls022;

pub fn translate_rustls_error(err: &(dyn std::error::Error + 'static)) -> Option<CertificateError> {
    if let Some(rustls::Error::InvalidCertificate(inner)) = err.downcast_ref::<rustls::Error>() {
        match inner {
            rustls::CertificateError::Expired => Some(CertificateError::CertificateValidationFailure {
                hint: "The server certificate has expired, the time it is being validated for is later than the certificate's `notAfter` time.".into(),
                msg: err.to_string()
            }),

            rustls::CertificateError::NotValidYet => Some(CertificateError::CertificateValidationFailure {
                hint: "The server certificate is not valid yet, the time it is being validated for is earlier than the certificate's `notBefore` time.".into(),
                msg: err.to_string(),
            }),

            _ => Some(CertificateError::CertificateValidationFailure {
                hint: "Server certificate validation error.".into(),
                msg: err.to_string(),
            }),
        }
    } else {
        None
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CertificateError {
    #[error("Could not access {path}: {error}")]
    IoError {
        path: PathBuf,
        error: std::io::Error,
    },

    #[error("Cryptography related error")]
    CryptographyError(#[from] rcgen::Error),

    #[error("PEM file format error")]
    PemError(#[from] x509_parser::error::PEMError),

    #[error("X509 file format error: {0}")]
    X509Error(String), // One cannot use x509_parser::error::X509Error unless one use `nom`.

    #[error("DeviceID Error")]
    InvalidDeviceID(#[from] DeviceIdError),

    #[error("Fail to parse the private key")]
    UnknownPrivateKeyFormat,

    #[error("Could not parse certificate {path}")]
    CertificateParseFailed {
        path: PathBuf,
        source: anyhow::Error,
    },

    #[error("HTTP Connection Problem: {msg} \nHint: {hint}")]
    CertificateValidationFailure { hint: String, msg: String },

    #[error("Failed to add the certificate to root store")]
    RootStoreAdd,

    #[error(transparent)]
    CertParse(anyhow::Error),
}

impl From<rustls::Error> for CertificateError {
    fn from(value: rustls::Error) -> Self {
        CertificateError::CertParse(value.into())
    }
}

impl From<rustls_022::Error> for CertificateError {
    fn from(value: rustls_022::Error) -> Self {
        CertificateError::CertParse(value.into())
    }
}
