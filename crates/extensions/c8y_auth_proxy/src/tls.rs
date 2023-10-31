use axum::middleware::AddExtension;
use axum::Extension;
use axum_server::accept::Accept;
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server::tls_rustls::RustlsConfig;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::future::BoxFuture;
use miette::miette;
use miette::Context;
use miette::IntoDiagnostic;
use rustls::server::AllowAnyAnonymousOrAuthenticatedClient;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::server::ClientCertVerifier;
use rustls::Certificate;
use rustls::PrivateKey;
use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use std::fs::File;
use std::io;
use std::io::BufReader;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio_rustls::server::TlsStream;
use tower::Layer;
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

#[derive(Debug, Clone)]
pub struct Acceptor {
    inner: RustlsAcceptor,
}

#[derive(Debug, Clone)]
pub struct TlsData {
    pub common_name: Option<Arc<str>>,
}

impl Acceptor {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            inner: RustlsAcceptor::new(RustlsConfig::from_config(Arc::new(config))),
        }
    }
}

impl<I, S> Accept<I, S> for Acceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = TlsStream<I>;
    type Service = AddExtension<S, TlsData>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let acceptor = self.inner.clone();

        Box::pin(async move {
            let (stream, service) = acceptor.accept(stream, service).await?;
            let server_conn = stream.get_ref().1;
            let cert =
                (|| X509Certificate::from_der(&server_conn.peer_certificates()?.first()?.0).ok())();
            let certificate_info = TlsData {
                common_name: common_name(cert.as_ref()).map(Arc::from),
            };
            let service = Extension(certificate_info).layer(service);

            Ok((stream, service))
        })
    }
}

pub fn common_name<'a>(cert: Option<&'a (&[u8], X509Certificate)>) -> Option<&'a str> {
    cert?.1.subject.iter_common_name().next()?.as_str().ok()
}

/// Load the SSL configuration for rustls
pub fn get_ssl_config(
    certificate_chain: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    ca_dir: Option<Utf8PathBuf>,
) -> miette::Result<ServerConfig> {
    // Trusted CA for client certificates
    let mut roots = RootCertStore::empty();
    let verifier = if let Some(ca_dir) = &ca_dir {
        let mut ders = Vec::new();
        for file in ca_dir
            .read_dir_utf8()
            .into_diagnostic()
            .wrap_err_with(|| format!("reading {ca_dir}"))?
        {
            let file = file.unwrap();
            // TODO cope with symlinked dirs
            if file.file_type().map_or(true, |file| file.is_dir()) {
                continue;
            }
            let mut path = ca_dir.clone().to_path_buf();
            path.push(file.file_name());
            let Ok(mut pem_file) = File::open(&path).map(BufReader::new) else {
                continue;
            };
            if let Some(value) = rustls_pemfile::certs(&mut pem_file)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading {path}"))
                .unwrap()
                .into_iter()
                .next()
            {
                ders.push(value);
            };
        }
        roots.add_parsable_certificates(&ders);
        Arc::new(AllowAnyAuthenticatedClient::new(roots)) as Arc<dyn ClientCertVerifier>
    } else {
        Arc::new(AllowAnyAnonymousOrAuthenticatedClient::new(roots))
    };

    let server_cert = certificate_chain.into_iter().map(Certificate).collect();
    let server_key = PrivateKey(key_der);

    ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(verifier)
        .with_single_cert(server_cert, server_key)
        .into_diagnostic()
        .wrap_err("invalid key or certificate")
}

/// Load the server certificate
pub fn load_cert(filename: &Utf8Path) -> miette::Result<Vec<Vec<u8>>> {
    let certfile = File::open(filename)
        .into_diagnostic()
        .with_context(|| format!("cannot open certificate file: {filename:?}"))?;
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .into_diagnostic()
        .with_context(|| format!("parsing PEM-encoded certificate from {filename:?}"))
}

/// Load the server private key
pub fn load_pkey(filename: &Utf8Path) -> miette::Result<Vec<u8>> {
    let keyfile = File::open(filename)
        .into_diagnostic()
        .with_context(|| format!("cannot open key file {filename:?}"))?;
    let mut reader = BufReader::new(keyfile);
    rustls_pemfile::read_one(&mut reader)
        .into_diagnostic()
        .wrap_err_with(|| format!("reading PEM-encoded private key from {filename:?}"))?
        .ok_or(miette!(
            "expected private key in {filename:?}, but found no PEM-encoded data"
        ))
        .and_then(|item| match item {
            Item::ECKey(key) | Item::PKCS8Key(key) | Item::RSAKey(key) => Ok(key),
            Item::Crl(_) => Err(miette!("expected private key in {filename}, found a CRL")),
            Item::X509Certificate(_) => Err(miette!(
                "expected private key in {filename:?}, found an X509 certificate"
            )),
            item => Err(miette!(
                "expected private key in {filename:?}, found an unknown PEM-encoded item: {item:?}"
            )),
        })
}
