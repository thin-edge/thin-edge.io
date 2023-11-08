use anyhow::anyhow;
use anyhow::Context;
use axum::http::uri::InvalidUriParts;
use axum::http::Request;
use axum::http::StatusCode;
use axum::middleware::AddExtension;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use axum::Extension;
use axum_server::accept::Accept;
use axum_server::accept::DefaultAcceptor;
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server::tls_rustls::RustlsConfig;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use futures::future::BoxFuture;
use hyper::http::uri::Authority;
use hyper::http::uri::Scheme;
use hyper::Uri;
use pin_project::pin_project;
use rustls::server::AllowAnyAuthenticatedClient;
use rustls::server::ClientCertVerified;
use rustls::server::ClientCertVerifier;
use rustls::Certificate;
use rustls::DistinguishedName;
use rustls::PrivateKey;
use rustls::RootCertStore;
use rustls::ServerConfig;
use rustls_pemfile::Item;
use std::fs::File;
use std::io;
use std::io::Error;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;
use std::time::SystemTime;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::BufReader;
use tokio::io::ReadBuf;
use tokio_rustls::server::TlsStream;
use tower::Layer;
use tracing::error;
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

#[derive(Debug, Clone)]
pub struct Acceptor {
    inner: RustlsAcceptor,
}

impl From<ServerConfig> for Acceptor {
    fn from(config: ServerConfig) -> Self {
        Self::new(config)
    }
}

#[derive(Debug, Clone)]
pub struct TlsData {
    pub common_name: Option<Arc<str>>,
    pub is_secure: bool,
}

impl Acceptor {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            inner: RustlsAcceptor::new(RustlsConfig::from_config(Arc::new(config))),
        }
    }
}

#[pin_project(project = PossibleTlsStreamProj)]
pub enum PossibleTlsStream<I> {
    Tls(#[pin] Box<TlsStream<BufReader<I>>>),
    Insecure(#[pin] BufReader<I>),
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncRead for PossibleTlsStream<I> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            PossibleTlsStreamProj::Tls(tls) => tls.poll_read(cx, buf),
            PossibleTlsStreamProj::Insecure(insecure) => insecure.poll_read(cx, buf),
        }
    }
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncWrite for PossibleTlsStream<I> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.project() {
            PossibleTlsStreamProj::Tls(tls) => tls.poll_write(cx, buf),
            PossibleTlsStreamProj::Insecure(insecure) => insecure.poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        match self.project() {
            PossibleTlsStreamProj::Tls(tls) => tls.poll_flush(cx),
            PossibleTlsStreamProj::Insecure(insecure) => insecure.poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        match self.project() {
            PossibleTlsStreamProj::Tls(tls) => tls.poll_shutdown(cx),
            PossibleTlsStreamProj::Insecure(insecure) => insecure.poll_shutdown(cx),
        }
    }
}

#[derive(Debug, Copy, Clone)]
/// An alternative to [AllowAnyAnonymousOrAuthenticatedClient](rustls::server::AllowAnyAnonymousOrAuthenticatedClient)
/// that doesn't attempt any client authentication
///
/// This prevents clients that are using certificates from having their connection rejected due to the
/// supplied certificate not being trusted
pub struct AllowAnyClient;

impl ClientCertVerifier for AllowAnyClient {
    fn offer_client_auth(&self) -> bool {
        false
    }

    fn client_auth_root_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _: &Certificate,
        _: &[Certificate],
        _: SystemTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        unimplemented!("Client certificate verification is not supported by {self:?}")
    }
}

impl<I, S> Accept<I, S> for Acceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = PossibleTlsStream<I>;
    type Service = AddExtension<S, TlsData>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let acceptor = self.inner.clone();

        Box::pin(async move {
            let mut stream = BufReader::new(stream);
            let first_bytes = stream.fill_buf().await?;

            // To handle HTTP and HTTPS requests from the same server, we have to just guess
            // which is being used. The best approximation I can come up with is that HTTP
            // requests have a header section that is valid ASCII, and HTTPS requests will
            // contain some binary data that won't be valid ASCII (or UTF-8). As we're dealing
            // with ASCII, splitting the string at the byte level is guaranteed not to split a
            // UTF-8 code point, so [..20] just gets the first 20 characters of the string
            // (assuming it is a valid ASCII sequence)
            if std::str::from_utf8(&first_bytes[..20]).is_ok() {
                let acceptor = DefaultAcceptor;
                let (stream, service) = acceptor.accept(stream, service).await?;
                let certificate_info = TlsData {
                    common_name: None,
                    is_secure: false,
                };

                let service = Extension(certificate_info).layer(service);
                Ok((PossibleTlsStream::Insecure(stream), service))
            } else {
                let (stream, service) = acceptor.accept(stream, service).await?;
                let server_conn = stream.get_ref().1;
                let cert = (|| {
                    X509Certificate::from_der(&server_conn.peer_certificates()?.first()?.0).ok()
                })();
                let certificate_info = TlsData {
                    common_name: common_name(cert.as_ref()).map(Arc::from),
                    is_secure: true,
                };
                let service = Extension(certificate_info).layer(service);

                Ok((PossibleTlsStream::Tls(Box::new(stream)), service))
            }
        })
    }
}

pub async fn redirect_http_to_https<B>(
    request: Request<B>,
) -> Result<Request<B>, HttpsRedirectResponse> {
    let tls_data = request
        .extensions()
        .get::<TlsData>()
        .ok_or(HttpsRedirectResponse::MissingTlsData)?;

    if tls_data.is_secure {
        Ok(request)
    } else {
        let host = request
            .headers()
            .get("host")
            .ok_or(HttpsRedirectResponse::MissingHostHeader)?
            .to_owned();
        let mut uri = request.uri().to_owned().into_parts();
        uri.scheme = Some(Scheme::HTTPS);
        uri.authority = Some(Authority::from_maybe_shared(host).unwrap());
        let uri = Uri::try_from(uri).map_err(HttpsRedirectResponse::FailedToCreateUri)?;
        Err(HttpsRedirectResponse::RedirectTo(uri.to_string()))
    }
}

#[derive(Debug)]
pub enum HttpsRedirectResponse {
    MissingTlsData,
    MissingHostHeader,
    FailedToCreateUri(InvalidUriParts),
    RedirectTo(String),
}

impl IntoResponse for HttpsRedirectResponse {
    fn into_response(self) -> Response {
        let internal_err = (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal error in tedge-mapper",
        );
        match self {
            Self::MissingTlsData => {
                error!(
                    "{} not set. Are you using the right acceptor ({})?",
                    std::any::type_name::<TlsData>(),
                    std::any::type_name::<Acceptor>()
                );
                internal_err.into_response()
            }
            Self::FailedToCreateUri(e) => {
                error!("{}", anyhow::Error::new(e).context("Failed to create URI"));
                internal_err.into_response()
            }
            Self::MissingHostHeader => (
                StatusCode::BAD_REQUEST,
                "This server does not support HTTP. Please retry this request using HTTPS",
            )
                .into_response(),
            Self::RedirectTo(uri) => Redirect::temporary(&uri).into_response(),
        }
        .into_response()
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
