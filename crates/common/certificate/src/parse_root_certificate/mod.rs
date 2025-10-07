use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls_pemfile::certs;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub use tedge_p11_server::AuthPin;
pub use tedge_p11_server::CryptokiConfig;
pub use tedge_p11_server::CryptokiConfigDirect;
pub use tedge_p11_server::SecretString;

use crate::CertificateError;

pub fn create_tls_config(
    root_certificates: impl AsRef<Path>,
    client_private_key: impl AsRef<Path>,
    client_certificate: impl AsRef<Path>,
) -> Result<ClientConfig, CertificateError> {
    let root_cert_store = new_root_store(root_certificates.as_ref())?;
    let pvt_key = read_pvt_key(client_private_key)?;
    let cert_chain = read_cert_chain(client_certificate)?;

    Ok(ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_client_auth_cert(cert_chain, pvt_key)?)
}

/// Create a TLS ClientConfig that uses a PKCS#11 device for client authentication.
///
/// This TLS configuration should be used for communication between a device (or bridge) and a cloud
/// remote MQTT broker, not local MQTT broker.
pub fn create_tls_config_cryptoki(
    root_certificates: impl AsRef<Path>,
    client_certificate: impl AsRef<Path>,
    cryptoki_config: CryptokiConfig,
) -> Result<ClientConfig, CertificateError> {
    use rustls::sign::CertifiedKey;
    use std::sync::Arc;
    use tedge_p11_server::single_cert_and_key::SingleCertAndKey;

    let root_cert_store = new_root_store(root_certificates.as_ref())?;
    let cert_chain = read_cert_chain(client_certificate)?;
    let key = tedge_p11_server::signing_key(cryptoki_config)?;

    let certified_key = CertifiedKey {
        cert: cert_chain,
        key: key.to_rustls_signing_key(),
        ocsp: None,
    };
    let resolver: SingleCertAndKey = certified_key.into();

    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_client_cert_resolver(Arc::new(resolver));

    Ok(config)
}

pub fn client_config_for_ca_certificates<P>(
    root_certificates: impl IntoIterator<Item = P>,
) -> Result<ClientConfig, std::io::Error>
where
    P: AsRef<Path>,
{
    let mut roots = RootCertStore::empty();
    for cert_path in root_certificates {
        rec_add_root_cert(&mut roots, cert_path.as_ref());
    }

    let (mut valid_count, mut invalid_count) = (0, 0);
    for cert in rustls_native_certs::load_native_certs().expect("could not load platform certs") {
        match roots.add(cert) {
            Ok(_) => valid_count += 1,
            Err(err) => {
                tracing::debug!("certificate parsing failed: {:?}", err);
                invalid_count += 1
            }
        }
    }
    tracing::debug!(
        "with_native_roots processed {} valid and {} invalid certs",
        valid_count,
        invalid_count
    );
    if roots.is_empty() {
        tracing::debug!("no valid root CA certificates found");
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no valid root CA certificates found ({invalid_count} invalid)"),
        ))?
    }

    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

pub fn create_tls_config_without_client_cert(
    root_certificates: impl AsRef<Path>,
) -> Result<ClientConfig, CertificateError> {
    let root_cert_store = new_root_store(root_certificates.as_ref())?;

    Ok(ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth())
}

pub fn add_certs_from_file(
    root_store: &mut RootCertStore,
    cert_file: impl AsRef<Path>,
) -> Result<(), CertificateError> {
    let cert_chain = read_cert_chain(cert_file)?;
    for cert in cert_chain {
        root_store
            .add(cert)
            .map_err(|_| CertificateError::RootStoreAdd)?;
    }

    Ok(())
}

pub fn add_certs_from_directory(
    root_store: &mut RootCertStore,
    cert_dir: impl AsRef<Path>,
) -> Result<(), CertificateError> {
    let cert_dir = cert_dir.as_ref();
    let files = fs::read_dir(cert_dir).map_err(|error| CertificateError::IoError {
        error,
        path: cert_dir.to_owned(),
    })?;
    let certs = files.filter_map(|f| f.ok()).filter(|file| {
        file.path()
            .extension()
            .filter(|&extension| {
                ["pem", "cer", "crt"]
                    .map(OsString::from)
                    .iter()
                    .any(|e| e == extension)
            })
            .is_some()
    });

    for cert_file in certs {
        add_certs_from_file(root_store, cert_file.path())?;
    }

    Ok(())
}

fn new_root_store(cert_path: &Path) -> Result<RootCertStore, CertificateError> {
    let mut root_store = RootCertStore::empty();
    rec_add_root_cert(&mut root_store, cert_path);
    Ok(root_store)
}

fn rec_add_root_cert(root_store: &mut RootCertStore, cert_path: &Path) {
    if let Err(err) = try_rec_add_root_cert(root_store, cert_path) {
        eprintln!("Ignoring certificates in {:?} due to: {}", cert_path, err)
    }
}

fn try_rec_add_root_cert(
    root_store: &mut RootCertStore,
    cert_path: &Path,
) -> Result<(), CertificateError> {
    let metadata = fs::metadata(cert_path).map_err(|error| CertificateError::IoError {
        error,
        path: cert_path.to_owned(),
    })?;
    if metadata.is_dir() {
        let entries = fs::read_dir(cert_path).map_err(|error| CertificateError::IoError {
            error,
            path: cert_path.to_owned(),
        })?;
        for file_entry in entries.flatten() {
            rec_add_root_cert(root_store, &file_entry.path());
        }
    } else if let Err(err) = add_root_cert(root_store, cert_path) {
        eprintln!(
            "Ignoring certificates in file {:?} due to: {}",
            cert_path, err
        )
    }
    Ok(())
}

fn add_root_cert(root_store: &mut RootCertStore, cert_path: &Path) -> Result<(), CertificateError> {
    let certificates = read_cert_chain(cert_path)?;
    for certificate in certificates.iter() {
        if let Err(err) = root_store.add(certificate.clone()) {
            eprintln!(
                "Ignoring certificate in file {:?} due to: {}",
                cert_path, err
            )
        }
    }
    Ok(())
}

pub fn read_pvt_key(
    key_file: impl AsRef<Path>,
) -> Result<PrivateKeyDer<'static>, CertificateError> {
    PrivateKeyDer::from_pem_file(&key_file).map_err(|err| {
        if let rustls::pki_types::pem::Error::Io(io) = err {
            CertificateError::IoError {
                path: key_file.as_ref().to_path_buf(),
                error: io,
            }
        } else {
            CertificateError::CertParse2(err)
        }
    })
}

pub fn read_cert_chain(
    cert_file: impl AsRef<Path>,
) -> Result<Vec<CertificateDer<'static>>, CertificateError> {
    let f = File::open(&cert_file).map_err(|error| CertificateError::IoError {
        error,
        path: cert_file.as_ref().to_owned(),
    })?;
    let mut cert_reader = BufReader::new(f);
    certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CertificateError::CertificateParseFailed {
            path: cert_file.as_ref().to_path_buf(),
            source: e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    #[test]
    fn parse_supported_key() {
        let key = concat!(
            "-----BEGIN RSA PRIVATE KEY-----\n",
            "MC4CAQ\n",
            "-----END RSA PRIVATE KEY-----"
        );
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(key.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        let parsed_key = read_pvt_key(temp_file.path()).unwrap();
        let expected_pvt_key = PrivateKeyDer::Pkcs1(vec![48, 46, 2, 1].into());
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
        temp_file.flush().unwrap();

        read_pvt_key(temp_file.path()).unwrap_err();
    }

    #[test]
    fn an_empty_directory_contains_no_root_certificate() {
        let temp_dir = TempDir::new().unwrap();

        let root_certs = new_root_store(temp_dir.path()).unwrap();
        assert!(root_certs.is_empty());
    }

    #[test]
    fn all_certificates_of_all_chains_are_root_certificates() {
        let temp_dir = TempDir::new().unwrap();

        // Add a first chain with 1 certificate
        fs::write(
            temp_dir.path().join("cert_1"),
            include_str!("../test_root_cert_1.txt"),
        )
        .unwrap();

        // Add a second chain with 2 certificates
        fs::write(
            temp_dir.path().join("cert_2"),
            include_str!("../test_root_cert_2.txt"),
        )
        .unwrap();

        let root_certs = new_root_store(temp_dir.path()).unwrap();
        assert_eq!(root_certs.len(), 3);
    }

    #[test]
    fn all_certificates_are_loaded_even_under_sub_directories() {
        let temp_dir = TempDir::new().unwrap();

        // Add a first chain with 1 certificate
        fs::write(
            temp_dir.path().join("cert_1"),
            include_str!("../test_root_cert_1.txt"),
        )
        .unwrap();

        // Add a second chain with 2 certificates in a sub directory
        fs::create_dir(temp_dir.path().join("sub_certs")).unwrap();
        fs::write(
            temp_dir.path().join("sub_certs/cert_2"),
            include_str!("../test_root_cert_2.txt"),
        )
        .unwrap();

        let root_certs = new_root_store(temp_dir.path()).unwrap();
        assert_eq!(root_certs.len(), 3);
    }
}
