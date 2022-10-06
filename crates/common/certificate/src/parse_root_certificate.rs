use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore};
use rustls_pemfile::{certs, pkcs8_private_keys, rsa_private_keys};
use std::{fs, fs::File, io::BufReader, path::PathBuf};

use crate::CertificateError;

pub fn create_tls_config(
    root_certificates: PathBuf,
    client_private_key: PathBuf,
    client_certificate: PathBuf,
) -> Result<ClientConfig, CertificateError> {
    let root_cert_store = new_root_store(root_certificates)?;
    let pvt_key = read_pvt_key(client_private_key)?;
    let cert_chain = read_cert_chain(client_certificate)?;

    Ok(ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_single_cert(cert_chain, pvt_key)?)
}

fn new_root_store(cert_path: PathBuf) -> Result<RootCertStore, CertificateError> {
    let mut root_store = RootCertStore::empty();
    if fs::metadata(&cert_path)?.is_dir() {
        for file_entry in fs::read_dir(cert_path)? {
            add_root_cert(&mut root_store, file_entry?.path())?;
        }
    } else {
        add_root_cert(&mut root_store, cert_path)?;
    }
    Ok(root_store)
}

fn add_root_cert(
    root_store: &mut RootCertStore,
    cert_path: PathBuf,
) -> Result<(), CertificateError> {
    let certificates = read_cert_chain(cert_path.clone())?;
    for certificate in certificates.iter() {
        if let Err(err) = root_store.add(certificate) {
            eprintln!("Ignore invalid certificate {:?} : {}", cert_path, err)
        }
    }
    Ok(())
}

fn read_pvt_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    parse_pkcs8_key(key_file.clone()).or_else(|_| parse_rsa_key(key_file))
}

fn parse_pkcs8_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match pkcs8_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(PrivateKey(key[0].clone())),
        _ => Err(CertificateError::UnknownPrivateKeyFormat),
    }
}

fn parse_rsa_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    let f = File::open(&key_file)?;
    let mut key_reader = BufReader::new(f);
    match rsa_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(PrivateKey(key[0].clone())),
        _ => Err(CertificateError::UnknownPrivateKeyFormat),
    }
}

pub fn read_cert_chain(cert_file: PathBuf) -> Result<Vec<Certificate>, CertificateError> {
    let f = File::open(cert_file)?;
    let mut cert_reader = BufReader::new(f);
    certs(&mut cert_reader)
        .map(|der_chain| der_chain.into_iter().map(Certificate).collect())
        .map_err(|_| CertificateError::CertificateParseFailed)
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
        let pvt_key = PrivateKey(vec![48, 46, 2, 1]);
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
        let pvt_key = PrivateKey(vec![48, 46, 2, 1]);
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
        let expected_pvt_key = PrivateKey(vec![48, 46, 2, 1]);
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
