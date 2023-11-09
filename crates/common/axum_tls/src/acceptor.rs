use crate::maybe_tls::MaybeTlsStream;

use axum::middleware::AddExtension;
use axum::Extension;
use axum_server::accept::Accept;
use axum_server::accept::DefaultAcceptor;
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server::tls_rustls::RustlsConfig;

use futures::future::BoxFuture;

use rustls::ServerConfig;

use std::io;
use std::sync::Arc;

use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::BufReader;
use tower::Layer;
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

/// An [axum_server::Acceptor] that accepts TLS connections via [rustls]
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
    type Stream = MaybeTlsStream<I>;
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
                Ok((MaybeTlsStream::Insecure(stream), service))
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

                Ok((MaybeTlsStream::Tls(Box::new(stream)), service))
            }
        })
    }
}

pub fn common_name<'a>(cert: Option<&'a (&[u8], X509Certificate)>) -> Option<&'a str> {
    cert?.1.subject.iter_common_name().next()?.as_str().ok()
}
