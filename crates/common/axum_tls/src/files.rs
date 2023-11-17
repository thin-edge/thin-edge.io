use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::Certificate;
use rustls::PrivateKey;
use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use std::fs::File;
use std::io;
use std::sync::Arc;

/// Read a directory into a RootCertStore
// TODO unit test me
pub fn read_trust_store(ca_dir: &Utf8Path) -> anyhow::Result<RootCertStore> {
    let mut roots = RootCertStore::empty();

    let mut ders = Vec::new();
    for file in ca_dir
        .read_dir_utf8()
        .with_context(|| format!("reading {ca_dir}"))?
    {
        let file = file.with_context(|| format!("reading metadata for file in {ca_dir}"))?;
        let mut path = ca_dir.to_path_buf();
        path.push(file.file_name());

        if path.is_dir() {
            continue;
        }

        let Ok(mut pem_file) = File::open(&path).map(std::io::BufReader::new) else {
            continue;
        };
        if let Some(value) = rustls_pemfile::certs(&mut pem_file)
            .with_context(|| format!("reading {path}"))?
            .into_iter()
            .next()
        {
            ders.push(value);
        };
    }
    roots.add_parsable_certificates(&ders);

    Ok(roots)
}

/// Load the SSL configuration for rustls
pub fn ssl_config(
    certificate_chain: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    root_certs: Option<RootCertStore>,
) -> anyhow::Result<ServerConfig> {
    // Trusted CA for client certificates
    let config = ServerConfig::builder().with_safe_defaults();

    let config = if let Some(root_certs) = root_certs {
        config.with_client_cert_verifier(Arc::new(AllowAnyAuthenticatedClient::new(root_certs)))
    } else {
        config.with_no_client_auth()
    };

    let server_cert = certificate_chain.into_iter().map(Certificate).collect();
    let server_key = PrivateKey(key_der);

    config
        .with_single_cert(server_cert, server_key)
        .context("invalid key or certificate")
}

/// Load the server certificate
pub fn load_cert(filename: &Utf8Path) -> anyhow::Result<Vec<Vec<u8>>> {
    let certfile = File::open(filename)
        .with_context(|| format!("cannot open certificate file: {filename:?}"))?;
    let mut reader = std::io::BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .with_context(|| format!("parsing PEM-encoded certificate from {filename:?}"))
}

/// Load the server private key
pub fn load_pkey(filename: &Utf8Path) -> anyhow::Result<Vec<u8>> {
    let keyfile =
        File::open(filename).with_context(|| format!("cannot open key file {filename:?}"))?;
    let mut reader = std::io::BufReader::new(keyfile);
    pkey_from_pem(&mut reader, filename)
}

pub fn pkey_from_pem(reader: &mut dyn io::BufRead, filename: &Utf8Path) -> anyhow::Result<Vec<u8>> {
    rustls_pemfile::read_one(reader)
        .with_context(|| format!("reading PEM-encoded private key from {filename:?}"))?
        .ok_or(anyhow!(
            "expected private key in {filename:?}, but found no PEM-encoded data"
        ))
        .and_then(|item| match item {
            Item::ECKey(key) | Item::PKCS8Key(key) | Item::RSAKey(key) => Ok(key),
            Item::Crl(_) => Err(anyhow!("expected private key in {filename:?}, found a CRL")),
            Item::X509Certificate(_) => Err(anyhow!(
                "expected private key in {filename:?}, found an X509 certificate"
            )),
            _item => Err(anyhow!(
                "expected private key in {filename:?}, found an unknown PEM-encoded item"
            )),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use axum::routing::get;
    use axum::Router;
    use std::io::Cursor;

    mod read_trust_store {
        use super::*;
        use std::os::unix::fs::symlink;
        use std::path::Path;
        use tempfile::tempdir;

        macro_rules! tempdir_path {
            ($dir:ident $(/ $file:literal)*) => ({
                let mut path = $dir.path().to_path_buf();
                $(path.push($file);)*
                ::camino::Utf8PathBuf::try_from(path).unwrap()
            })
        }

        #[test]
        fn reads_certificates_in_provided_directory() {
            let dir = tempdir().unwrap();

            copy_test_file_to("ec.crt", tempdir_path!(dir / "cert.crt")).unwrap();

            let store = read_trust_store(dir.path().try_into().unwrap()).unwrap();
            assert_eq!(store.len(), 1);
        }

        #[test]
        fn reads_multiple_certificates_from_directory() {
            let dir = tempdir().unwrap();

            copy_test_file_to("ec.crt", tempdir_path!(dir / "ec.crt")).unwrap();
            copy_test_file_to("rsa.crt", tempdir_path!(dir / "rsa.crt")).unwrap();

            let store = read_trust_store(dir.path().try_into().unwrap()).unwrap();
            assert_eq!(store.len(), 2);
        }

        #[test]
        fn ignores_private_keys_in_provided_directory() {
            let dir = tempdir().unwrap();
            copy_test_file_to("ec.key", tempdir_path!(dir / "example.key")).unwrap();

            let store = read_trust_store(dir.path().try_into().unwrap()).unwrap();
            assert_eq!(store.len(), 0);
        }

        #[test]
        fn reads_certificates_via_relative_symlink() {
            let dir = tempdir().unwrap();
            let cert_path = tempdir_path!(dir / "certs" / "cert.crt");
            let real_cert_path = tempdir_path!(dir / "actual_certs" / "cert.crt");

            let trust_store = create_parent_dir(&cert_path).unwrap();
            create_parent_dir(&real_cert_path).unwrap();
            copy_test_file_to("ec.crt", real_cert_path).unwrap();
            symlink("../actual_certs/cert.crt", &cert_path).unwrap();

            let store = read_trust_store(trust_store).unwrap();
            assert!(std::fs::symlink_metadata(&cert_path).unwrap().is_symlink());
            assert_eq!(store.len(), 1);
        }

        #[test]
        fn reads_certificates_via_absolute_symlink() {
            let dir = tempdir().unwrap();
            let cert_path = tempdir_path!(dir / "certs" / "cert.crt");
            let real_cert_path = tempdir_path!(dir / "actual_certs" / "cert.crt");

            let trust_store = create_parent_dir(&cert_path).unwrap();
            create_parent_dir(&real_cert_path).unwrap();
            copy_test_file_to("ec.crt", &real_cert_path).unwrap();
            symlink(&real_cert_path, &cert_path).unwrap();

            let store = read_trust_store(trust_store).unwrap();
            assert!(std::fs::symlink_metadata(&cert_path).unwrap().is_symlink());
            assert_eq!(store.len(), 1);
        }

        fn create_parent_dir(path: &Utf8Path) -> io::Result<&Utf8Path> {
            let path = path.parent().expect("path should have parent");
            std::fs::create_dir(path)?;
            Ok(path)
        }

        fn copy_test_file_to(test_file: &str, path: impl AsRef<Path>) -> io::Result<u64> {
            std::fs::copy(format!("./test_data/{test_file}"), path)
        }
    }

    #[test]
    fn load_pkey_fails_when_given_x509_certificate() {
        assert_eq!(
            load_pkey(Utf8Path::new("./test_data/ec.crt"))
                .unwrap_err()
                .to_string(),
            "expected private key in \"./test_data/ec.crt\", found an X509 certificate"
        );
    }

    #[test]
    fn load_pkey_fails_when_given_certificate_revocation_list() {
        assert_eq!(
            load_pkey(Utf8Path::new("./test_data/demo.crl"))
                .unwrap_err()
                .to_string(),
            "expected private key in \"./test_data/demo.crl\", found a CRL"
        );
    }

    mod server_accepts {
        use super::*;

        #[tokio::test]
        async fn alg_ed25519_pkcs8() {
            let key = test_data("ed25519.key");
            let cert = test_data("ed25519.crt");

            let (config, cert) = config_from_pem(&key, &cert).unwrap();

            assert_matches!(parse_key_to_item(&key), Item::PKCS8Key(_));
            assert_server_works_with(config, cert).await;
        }

        #[tokio::test]
        async fn alg_ec() {
            let key = test_data("ec.key");
            let cert = test_data("ec.crt");

            let (config, cert) = config_from_pem(&key, &cert).unwrap();

            assert_matches!(parse_key_to_item(&key), Item::ECKey(_));
            assert_server_works_with(config, cert).await;
        }

        #[tokio::test]
        async fn alg_ec_pkcs8() {
            let key = test_data("ec.pkcs8.key");
            let cert = test_data("ec.crt");

            let (config, cert) = config_from_pem(&key, &cert).unwrap();

            assert_matches!(parse_key_to_item(&key), Item::PKCS8Key(_));
            assert_server_works_with(config, cert).await;
        }

        #[tokio::test]
        async fn alg_rsa_pkcs8() {
            let key = test_data("rsa.pkcs8.key");
            let cert = test_data("rsa.crt");

            let (config, cert) = config_from_pem(&key, &cert).unwrap();

            assert_matches!(parse_key_to_item(&key), Item::PKCS8Key(_));
            assert_server_works_with(config, cert).await;
        }

        #[tokio::test]
        async fn alg_rsa_pkcs1() {
            let key = test_data("rsa.pkcs1.key");
            let cert = test_data("rsa.crt");

            let (config, cert) = config_from_pem(&key, &cert).unwrap();

            assert_matches!(parse_key_to_item(&key), Item::RSAKey(_));
            assert_server_works_with(config, cert).await;
        }

        fn parse_key_to_item(pem: &str) -> Item {
            rustls_pemfile::read_one(&mut Cursor::new(pem))
                .unwrap()
                .unwrap()
        }

        fn test_data(file_name: &str) -> String {
            std::fs::read_to_string(format!("./test_data/{file_name}"))
                .with_context(|| format!("opening file {file_name} from test_data"))
                .unwrap()
        }

        fn config_from_pem(
            key: &str,
            cert: &str,
        ) -> anyhow::Result<(ServerConfig, reqwest::tls::Certificate)> {
            let chain = rustls_pemfile::certs(&mut Cursor::new(cert)).context("reading certs")?;
            let key_der = parse_key_to_der(key)?;
            let cert = reqwest::tls::Certificate::from_der(
                chain.first().expect("chain should contain certificate"),
            )
            .context("converting certificate to reqwest::tls::Certificate")?;
            let config = ssl_config(chain, key_der, None)?;

            Ok((config, cert))
        }

        fn parse_key_to_der(pem: &str) -> anyhow::Result<Vec<u8>> {
            pkey_from_pem(
                &mut Cursor::new(pem),
                Utf8Path::new("just-in-memory-not-a-file.pem"),
            )
            .context("calling pkey_from_pem")
        }

        async fn assert_server_works_with(config: ServerConfig, cert: reqwest::tls::Certificate) {
            let (port, listener) = listener();
            let app = Router::new().route("/test", get(|| async { "it works!" }));

            let task = tokio::spawn(crate::start_tls_server(listener, config, app));
            let client = reqwest::Client::builder()
                .add_root_certificate(cert)
                .build()
                .unwrap();
            assert_eq!(
                client
                    .get(format!("https://localhost:{port}/test"))
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap(),
                "it works!"
            );
            task.abort();
        }

        fn listener() -> (u16, std::net::TcpListener) {
            let mut port = 3500;
            loop {
                if let Ok(listener) = std::net::TcpListener::bind(format!("127.0.0.1:{port}")) {
                    break (port, listener);
                }
                port += 1;
            }
        }
    }
}
