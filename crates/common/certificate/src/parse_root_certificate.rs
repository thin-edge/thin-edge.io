use rustls::Certificate;
use rustls::ClientConfig;
use rustls::PrivateKey;
use rustls::RootCertStore;
use rustls_pemfile::certs;
use rustls_pemfile::pkcs8_private_keys;
use rustls_pemfile::rsa_private_keys;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

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
    rec_add_root_cert(&mut root_store, cert_path);
    Ok(root_store)
}

fn rec_add_root_cert(root_store: &mut RootCertStore, cert_path: PathBuf) {
    if let Err(err) = try_rec_add_root_cert(root_store, cert_path.clone()) {
        eprintln!("Ignoring certificates in {:?} due to: {}", cert_path, err)
    }
}

fn try_rec_add_root_cert(
    root_store: &mut RootCertStore,
    cert_path: PathBuf,
) -> Result<(), CertificateError> {
    if fs::metadata(&cert_path)?.is_dir() {
        for file_entry in fs::read_dir(cert_path)?.flatten() {
            rec_add_root_cert(root_store, file_entry.path());
        }
    } else if let Err(err) = add_root_cert(root_store, cert_path.clone()) {
        eprintln!(
            "Ignoring certificates in file {:?} due to: {}",
            cert_path, err
        )
    }
    Ok(())
}

fn add_root_cert(
    root_store: &mut RootCertStore,
    cert_path: PathBuf,
) -> Result<(), CertificateError> {
    let certificates = read_cert_chain(cert_path.clone())?;
    for certificate in certificates.iter() {
        if let Err(err) = root_store.add(certificate) {
            eprintln!(
                "Ignoring certificate in file {:?} due to: {}",
                cert_path, err
            )
        }
    }
    Ok(())
}

fn read_pvt_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    parse_pkcs8_key(key_file.clone()).or_else(|_| parse_rsa_key(key_file))
}

fn parse_pkcs8_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    let f = File::open(key_file)?;
    let mut key_reader = BufReader::new(f);
    match pkcs8_private_keys(&mut key_reader) {
        Ok(key) if !key.is_empty() => Ok(PrivateKey(key[0].clone())),
        _ => Err(CertificateError::UnknownPrivateKeyFormat),
    }
}

fn parse_rsa_key(key_file: PathBuf) -> Result<PrivateKey, CertificateError> {
    let f = File::open(key_file)?;
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
    use tempfile::TempDir;

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

    #[test]
    fn an_empty_directory_contains_no_root_certificate() {
        let temp_dir = TempDir::new().unwrap();

        let root_certs = new_root_store(temp_dir.into_path()).unwrap();
        assert!(root_certs.is_empty());
    }

    #[test]
    fn all_certificates_of_all_chains_are_root_certificates() {
        let temp_dir = TempDir::new().unwrap();

        // Add a first chain with 1 certificate
        let mut cert_1 = File::create(temp_dir.path().join("cert_1")).unwrap();
        cert_1
            .write_all(include_str!("./test_root_cert_1.txt").as_bytes())
            .unwrap();

        // Add a second chain with 2 certificates
        let mut cert_2 = File::create(temp_dir.path().join("cert_2")).unwrap();
        cert_2
            .write_all(include_str!("./test_root_cert_2.txt").as_bytes())
            .unwrap();

        let root_certs = new_root_store(temp_dir.path().to_path_buf()).unwrap();
        assert_eq!(root_certs.len(), 3);
    }

    #[test]
    fn all_certificates_are_loaded_even_under_sub_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Add a first chain with 1 certificate
        let mut cert_1 = File::create(temp_dir.path().join("cert_1")).unwrap();
        cert_1
            .write_all(include_str!("./test_root_cert_1.txt").as_bytes())
            .unwrap();

        // Add a second chain with 2 certificates in a sub directory
        fs::create_dir(temp_dir.path().join("sub_certs")).unwrap();
        let mut cert_2 = File::create(temp_dir.path().join("sub_certs/cert_2")).unwrap();
        cert_2
            .write_all(include_str!("./test_root_cert_2.txt").as_bytes())
            .unwrap();

        let root_certs = new_root_store(temp_dir.path().to_path_buf()).unwrap();
        assert_eq!(root_certs.len(), 3);
    }
}
