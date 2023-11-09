use crate::verifier::AllowAnyClient;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::server::ClientCertVerifier;
use rustls::Certificate;
use rustls::PrivateKey;
use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use std::fs::File;
use std::sync::Arc;

/// Load the SSL configuration for rustls
pub fn ssl_config(
    certificate_chain: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    ca_dir: Option<Utf8PathBuf>,
) -> anyhow::Result<ServerConfig> {
    // Trusted CA for client certificates
    let mut roots = RootCertStore::empty();
    let verifier = if let Some(ca_dir) = &ca_dir {
        let mut ders = Vec::new();
        for file in ca_dir
            .read_dir_utf8()
            .with_context(|| format!("reading {ca_dir}"))?
        {
            let file = file.with_context(|| format!("reading metadata for file in {ca_dir}"))?;
            let mut path = ca_dir.clone().to_path_buf();
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
        Arc::new(AllowAnyAuthenticatedClient::new(roots)) as Arc<dyn ClientCertVerifier>
    } else {
        Arc::new(AllowAnyClient)
    };

    let server_cert = certificate_chain.into_iter().map(Certificate).collect();
    let server_key = PrivateKey(key_der);

    ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(verifier)
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
    rustls_pemfile::read_one(&mut reader)
        .with_context(|| format!("reading PEM-encoded private key from {filename:?}"))?
        .ok_or(anyhow!(
            "expected private key in {filename:?}, but found no PEM-encoded data"
        ))
        .and_then(|item| match item {
            // TODO test that all these keys actually work
            Item::ECKey(key) | Item::PKCS8Key(key) | Item::RSAKey(key) => Ok(key),
            Item::Crl(_) => Err(anyhow!("expected private key in {filename}, found a CRL")),
            Item::X509Certificate(_) => Err(anyhow!(
                "expected private key in {filename:?}, found an X509 certificate"
            )),
            item => Err(anyhow!(
                "expected private key in {filename:?}, found an unknown PEM-encoded item: {item:?}"
            )),
        })
}
