use rustls_0_19::{
    internal::pemfile::{certs, pkcs8_private_keys, rsa_private_keys},
    ClientConfig,
};
use std::{fs, fs::File, io::BufReader, path::PathBuf};

use crate::CertificateError;
use std::io::{Error, ErrorKind};

pub fn create_tls_config() -> rustls_0_19::ClientConfig {
    ClientConfig::new()
}

pub fn load_root_certs(
    root_store: &mut rustls_0_19::RootCertStore,
    cert_path: PathBuf,
) -> Result<(), CertificateError> {
    if fs::metadata(&cert_path)?.is_dir() {
        for file_entry in fs::read_dir(cert_path)? {
            add_root_cert(root_store, file_entry?.path())?;
        }
    } else {
        add_root_cert(root_store, cert_path)?;
    }
    Ok(())
}

pub fn add_root_cert(
    root_store: &mut rustls_0_19::RootCertStore,
    cert_path: PathBuf,
) -> Result<(), CertificateError> {
    let f = File::open(cert_path)?;
    let mut rd = BufReader::new(f);
    let _ = root_store.add_pem_file(&mut rd).map(|_| ()).map_err(|()| {
        Error::new(
            ErrorKind::InvalidData,
            "could not load PEM file".to_string(),
        )
    });
    Ok(())
}

pub fn read_pvt_key(key_file: PathBuf) -> Result<rustls_0_19::PrivateKey, CertificateError> {
    parse_pkcs8_key(key_file.clone()).or_else(|_| parse_rsa_key(key_file))
}

pub fn parse_pkcs8_key(key_file: PathBuf) -> Result<rustls_0_19::PrivateKey, CertificateError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match pkcs8_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(key[0].clone()),
        _ => Err(CertificateError::UnknownPrivateKeyFormat),
    }
}

pub fn parse_rsa_key(key_file: PathBuf) -> Result<rustls_0_19::PrivateKey, CertificateError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match rsa_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(key[0].clone()),
        _ => Err(CertificateError::UnknownPrivateKeyFormat),
    }
}

pub fn read_cert_chain(
    cert_file: PathBuf,
) -> Result<Vec<rustls_0_19::Certificate>, CertificateError> {
    let f = File::open(cert_file)?;
    let mut cert_reader = BufReader::new(f);
    certs(&mut cert_reader).map_err(|_| CertificateError::CertificateParseFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parse_private_rsa_key() {
        let key = concat!(
            "-----BEGIN RSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END RSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let result = parse_rsa_key(temp_file.path().into()).unwrap();
        let pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(result, pvt_key);
    }

    #[test]
    fn parse_private_pkcs8_key() {
        let key = concat! {
        "-----BEGIN PRIVATE KEY-----\n",
        "MC4CAQ\n",
        "-----END PRIVATE KEY-----"};
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let result = parse_pkcs8_key(temp_file.path().into()).unwrap();
        let pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(result, pvt_key);
    }

    #[test]
    fn parse_supported_key() {
        let key = concat!(
            "-----BEGIN RSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END RSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let parsed_key = read_pvt_key(temp_file.path().into()).unwrap();
        let expected_pvt_key = rustls_0_19::PrivateKey(vec![48, 46, 2, 1]);
        assert_eq!(parsed_key, expected_pvt_key);
    }

    #[test]
    fn parse_unsupported_key() {
        let key = concat!(
            "-----BEGIN DSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END DSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        let err = read_pvt_key(temp_file.path().into()).unwrap_err();
        assert!(matches!(err, CertificateError::UnknownPrivateKeyFormat));
    }
}
