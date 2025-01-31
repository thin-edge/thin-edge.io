use crate::maybe_tls::MaybeTlsStream;

use axum::middleware::AddExtension;
use axum::Extension;
use axum_server::accept::Accept;
use axum_server::accept::DefaultAcceptor;
use axum_server::tls_rustls::RustlsAcceptor;
use axum_server::tls_rustls::RustlsConfig;

use futures::future::BoxFuture;

use tokio_rustls::rustls::ServerConfig;

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
/// An [Acceptor](Accept) that accepts TLS connections via [rustls], or non TLS connections
pub struct Acceptor {
    inner: RustlsAcceptor,
}

impl From<ServerConfig> for Acceptor {
    fn from(config: ServerConfig) -> Self {
        Self::new(config)
    }
}

#[derive(Debug, Clone)]
/// [Extension] data added to a request by [Acceptor]
pub struct TlsData {
    /// The common name of the certificate used, if a client certificate was used
    pub common_name: Option<Arc<str>>,
    /// Whether the incoming request was made over HTTPS (`true`) or HTTP (`false`)
    pub is_secure: bool,
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
                let cert =
                    (|| X509Certificate::from_der(server_conn.peer_certificates()?.first()?).ok())(
                    );
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

fn common_name<'a>(cert: Option<&'a (&[u8], X509Certificate)>) -> Option<&'a str> {
    cert?.1.subject.iter_common_name().next()?.as_str().ok()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::ssl_config;
    use axum::http::uri::Scheme;
    use axum::routing::get;
    use axum::Router;
    use reqwest::Certificate;
    use reqwest::Client;
    use reqwest::Identity;
    use rustls::pki_types::pem::PemObject as _;
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::PrivateKeyDer;
    use rustls::RootCertStore;
    use std::net::SocketAddr;
    use std::net::TcpListener;

    #[tokio::test]
    async fn acceptor_accepts_non_tls_connections() {
        let server = Server::without_trusted_roots();
        let client = Client::new();

        assert_eq!(
            server.get_with_scheme(Scheme::HTTP, &client).await.unwrap(),
            "server is working"
        );
    }

    #[tokio::test]
    async fn acceptor_accepts_tls_connections() {
        let server = Server::without_trusted_roots();
        let client = Client::builder()
            .add_root_certificate(server.certificate.clone())
            .build()
            .unwrap();

        assert_eq!(
            server
                .get_with_scheme(Scheme::HTTPS, &client)
                .await
                .unwrap(),
            "server is working"
        );
    }

    #[tokio::test]
    async fn acceptor_ignores_client_certificates_when_authentication_is_disabled() {
        let server = Server::without_trusted_roots();
        let client = Client::builder()
            .add_root_certificate(server.certificate.clone())
            .identity(identity_with_name("my-client"))
            .build()
            .unwrap();

        assert_eq!(
            server
                .get_with_scheme(Scheme::HTTPS, &client)
                .await
                .unwrap(),
            "server is working"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn acceptor_rejects_untrusted_client_certificates() {
        let permitted_certificate =
            rcgen::generate_simple_self_signed(vec!["not-my-client".into()]).unwrap();
        let mut roots = RootCertStore::empty();
        roots
            .add(permitted_certificate.serialize_der().unwrap().into())
            .unwrap();
        let server = Server::with_trusted_roots(roots);
        let client = Client::builder()
            .add_root_certificate(server.certificate.clone())
            .identity(identity_with_name("my-client"))
            .build()
            .unwrap();

        let err = server
            .get_with_scheme(Scheme::HTTPS, &client)
            .await
            .unwrap_err();
        println!("{}", err);
        crate::error_matching::assert_error_matches(err, rustls::AlertDescription::UnknownCA);
    }

    #[tokio::test]
    async fn acceptor_rejects_connection_without_certificate() {
        let permitted_certificate =
            rcgen::generate_simple_self_signed(vec!["not-my-client".into()]).unwrap();
        let mut roots = RootCertStore::empty();
        roots
            .add(permitted_certificate.serialize_der().unwrap().into())
            .unwrap();
        let server = Server::with_trusted_roots(roots);
        let client = Client::builder()
            .add_root_certificate(server.certificate.clone())
            .build()
            .unwrap();

        let err = server
            .get_with_scheme(Scheme::HTTPS, &client)
            .await
            .unwrap_err();
        println!("{}", err);
        crate::error_matching::assert_error_matches(
            err,
            rustls::AlertDescription::CertificateRequired,
        );
    }

    #[tokio::test]
    async fn acceptor_accepts_trusted_client_certificates() {
        let client_cert = rcgen::generate_simple_self_signed(["my-client".into()]).unwrap();
        let identity = identity_from(&client_cert);
        let mut cert_store = RootCertStore::empty();
        cert_store.add_parsable_certificates([CertificateDer::from(
            client_cert.serialize_der().unwrap(),
        )]);

        let server = Server::with_trusted_roots(cert_store);
        let client = Client::builder()
            .add_root_certificate(server.certificate.clone())
            .identity(identity)
            .build()
            .unwrap();

        assert_eq!(
            server
                .get_with_scheme(Scheme::HTTPS, &client)
                .await
                .unwrap(),
            "server is working"
        );
    }

    struct Server {
        certificate: Certificate,
        port: u16,
    }

    fn identity_with_name(name: &str) -> Identity {
        let client_cert = rcgen::generate_simple_self_signed([name.into()]).unwrap();
        identity_from(&client_cert)
    }

    fn identity_from(cert: &rcgen::Certificate) -> Identity {
        let mut pem = cert.serialize_private_key_pem().into_bytes();
        pem.append(&mut cert.serialize_pem().unwrap().into_bytes());
        Identity::from_pem(&pem).unwrap()
    }

    impl Server {
        async fn get_with_scheme(
            &self,
            protocol: Scheme,
            client: &Client,
        ) -> reqwest::Result<String> {
            let uri = format!("{protocol}://localhost:{}/test", self.port);
            client
                .get(uri)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await
        }

        fn without_trusted_roots() -> Self {
            Self::start(None)
        }

        fn with_trusted_roots(root_cert_store: RootCertStore) -> Self {
            Self::start(Some(root_cert_store))
        }

        fn start(trusted_roots: Option<RootCertStore>) -> Self {
            let mut port = 3000;
            let listener = loop {
                if let Ok(listener) = TcpListener::bind::<SocketAddr>(([127, 0, 0, 1], port).into())
                {
                    break listener;
                }
                port += 1;
            };
            let certificate = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
            let certificate_der = CertificateDer::from(certificate.serialize_der().unwrap());
            let private_key_der =
                PrivateKeyDer::from_pem_slice(certificate.serialize_private_key_pem().as_bytes())
                    .unwrap();
            let certificate = reqwest::Certificate::from_der(&certificate_der).unwrap();
            let config = ssl_config(vec![certificate_der], private_key_der, trusted_roots).unwrap();
            tokio::spawn(
                axum_server::from_tcp(listener)
                    .acceptor(Acceptor::from(config.clone()))
                    .serve(
                        Router::new()
                            .route("/test", get(|| async { "server is working" }))
                            .into_make_service(),
                    ),
            );

            Self { port, certificate }
        }
    }
}
