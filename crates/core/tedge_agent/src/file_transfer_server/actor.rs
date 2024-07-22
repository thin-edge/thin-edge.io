use crate::file_transfer_server::error::FileTransferError;
use crate::file_transfer_server::http_rest::http_file_transfer_server;
use anyhow::Context;
use async_trait::async_trait;
use axum_tls::config::load_ssl_config;
use axum_tls::config::PemReader;
use axum_tls::config::TrustStoreLoader;
use camino::Utf8PathBuf;
use rustls::ServerConfig;
use std::convert::Infallible;
use std::net::SocketAddr;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_config::OptionalConfig;
use tokio::net::TcpListener;
use tracing::log::info;

pub struct FileTransferServerActor {
    file_transfer_dir: Utf8PathBuf,
    rustls_config: Option<ServerConfig>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    listener: TcpListener,
}

#[derive(Debug, Clone)]
// In the tests, CertKeyPath is replaced with a String, and CaPath is replaced with a RootCertStore
// hence they need to be separate types
pub(crate) struct FileTransferServerConfig<CertKeyPath = Utf8PathBuf, CaPath = Utf8PathBuf> {
    pub file_transfer_dir: Utf8PathBuf,
    pub cert_path: OptionalConfig<CertKeyPath>,
    pub key_path: OptionalConfig<CertKeyPath>,
    pub ca_path: OptionalConfig<CaPath>,
    pub bind_addr: SocketAddr,
}

/// HTTP file transfer server is stand-alone.
#[async_trait]
impl Actor for FileTransferServerActor {
    fn name(&self) -> &str {
        "HttpFileTransferServer"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let server =
            http_file_transfer_server(self.listener, self.file_transfer_dir, self.rustls_config)?;

        tokio::select! {
            result = server => {
                info!("Done");
                return Ok(result.map_err(FileTransferError::FromIo)?);
            }
            Some(RuntimeRequest::Shutdown) = self.signal_receiver.next() => {
                info!("Shutdown");
                return Ok(());
            }
        }
    }
}

pub struct FileTransferServerBuilder {
    file_transfer_dir: Utf8PathBuf,
    rustls_config: Option<ServerConfig>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    listener: TcpListener,
}

impl FileTransferServerBuilder {
    pub(crate) async fn try_bind(
        config: FileTransferServerConfig<impl PemReader, impl TrustStoreLoader>,
    ) -> Result<Self, anyhow::Error> {
        let listener = TcpListener::bind(config.bind_addr)
            .await
            .with_context(|| format!("Binding file-transfer server to {}", config.bind_addr))?;
        let (signal_sender, signal_receiver) = mpsc::channel(10);

        Ok(Self {
            rustls_config: load_ssl_config(
                config.cert_path,
                config.key_path,
                config.ca_path,
                "File transfer service",
            )?,
            file_transfer_dir: config.file_transfer_dir,
            signal_sender,
            signal_receiver,
            listener,
        })
    }
}

impl RuntimeRequestSink for FileTransferServerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        Box::new(self.signal_sender.clone())
    }
}

impl Builder<FileTransferServerActor> for FileTransferServerBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<FileTransferServerActor, Self::Error> {
        Ok(FileTransferServerActor {
            file_transfer_dir: self.file_transfer_dir,
            rustls_config: self.rustls_config,
            signal_receiver: self.signal_receiver,
            listener: self.listener,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;
    use axum_tls::config::InjectedValue;
    use camino::Utf8PathBuf;
    use mpsc::Receiver;
    use mpsc::Sender;
    use reqwest::Certificate;
    use reqwest::Identity;
    use rustls::RootCertStore;
    use tedge_api::path::DataDir;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::fs;

    #[tokio::test]
    async fn http_server_put_and_get() -> anyhow::Result<()> {
        let server = TestFileTransferService::new_http().await?;
        let file_name = "test-file";
        let test_url = server.url_for(file_name);

        let client = server.client();

        let upload_response = client.put(&test_url).body("file").send().await.unwrap();
        assert_eq!(upload_response.status(), hyper::StatusCode::CREATED);

        // Check if a file is created.
        let file_path = server.temp_path_for(file_name);
        let file_content = fs::read_to_string(file_path)
            .await
            .with_context(|| format!("reading file {file_name:?}"))?;
        assert_eq!(file_content, "file");

        let get_response = client.get(&test_url).send().await.unwrap();
        assert_eq!(get_response.status(), hyper::StatusCode::OK);

        Ok(())
    }

    #[tokio::test]
    async fn check_server_does_not_panic_when_port_is_in_use() -> anyhow::Result<()> {
        let ttd = TempTedgeDir::new();
        let (_listener, port_in_use) = create_listener().await?;

        let binding_res = FileTransferServerBuilder::try_bind(http_config(&ttd, port_in_use)).await;

        ensure!(
            binding_res.is_err(),
            "expected port binding to fail, but `try_bind` finished successfully"
        );

        Ok(())
    }

    #[tokio::test]
    async fn server_uses_https_if_certificate_is_configured() -> anyhow::Result<()> {
        let server_cert = rcgen::generate_simple_self_signed(["localhost".into()])
            .context("generating server certificate")?;
        let server = TestFileTransferService::new_https(server_cert, None).await?;
        let test_url = server.url_for("test-file");

        let client = server.anonymous_client()?;
        let upload_response = client.put(&test_url).body("file").send().await.unwrap();
        assert_eq!(upload_response.status(), hyper::StatusCode::CREATED);

        Ok(())
    }

    #[tokio::test]
    async fn server_accepts_connections_with_trusted_root_certificates() -> anyhow::Result<()> {
        let server_cert = rcgen::generate_simple_self_signed(["localhost".into()])
            .context("generating server certificate")?;
        let client_cert = rcgen::generate_simple_self_signed(["a-client".into()])
            .context("generating client certificate")?;
        let server = TestFileTransferService::new_https(server_cert, Some(&client_cert)).await?;
        let test_url = server.url_for("test-file");

        let client = server.client_with_certificate(&client_cert)?;
        let upload_response = client.put(&test_url).body("file").send().await.unwrap();
        assert_eq!(upload_response.status(), hyper::StatusCode::CREATED);

        Ok(())
    }

    #[tokio::test]
    async fn server_rejects_unauthenticated_connections_if_configured() -> anyhow::Result<()> {
        let server_cert = rcgen::generate_simple_self_signed(["localhost".into()])
            .context("generating server certificate")?;
        let client_cert = rcgen::generate_simple_self_signed(["a-client".into()])
            .context("generating client certificate")?;
        let server = TestFileTransferService::new_https(server_cert, Some(&client_cert)).await?;

        let client = server.anonymous_client()?;
        let test_url = server.url_for("test/file");

        let upload_err = client.put(&test_url).body("file").send().await.unwrap_err();
        axum_tls::assert_error_matches(upload_err, rustls::AlertDescription::CertificateRequired);

        Ok(())
    }

    /// A wrapper around a running [FileTransferServiceActor] to simplify/clarify test code
    struct TestFileTransferService<Cert> {
        port: u16,
        temp_dir: TempTedgeDir,
        server_cert: Cert,
        server_err: Receiver<RuntimeError>,
    }

    impl TestFileTransferService<()> {
        async fn new_http() -> anyhow::Result<Self> {
            let temp_dir = TempTedgeDir::new();
            let config = http_config(&temp_dir, 0);
            let (tx, rx) = mpsc::channel(1);
            let port = Self::spawn(config, tx).await?;

            Ok(TestFileTransferService {
                port,
                temp_dir,
                server_cert: (),
                server_err: rx,
            })
        }

        fn url_for(&self, path: &str) -> String {
            format!("http://localhost:{}/tedge/file-transfer/{path}", self.port)
        }

        #[allow(clippy::disallowed_methods)]
        fn client(&self) -> reqwest::Client {
            reqwest::Client::new()
        }
    }

    impl<C> Drop for TestFileTransferService<C> {
        fn drop(&mut self) {
            if let Ok(Some(value)) = self.server_err.try_next() {
                if std::thread::panicking() {
                    println!("Error running server: {value}")
                } else {
                    Err(value).context("Error running server").unwrap()
                }
            }
        }
    }

    impl TestFileTransferService<rcgen::Certificate> {
        async fn new_https(
            server_cert: rcgen::Certificate,
            trusted_root: Option<&rcgen::Certificate>,
        ) -> anyhow::Result<Self> {
            let temp_dir = TempTedgeDir::new();
            let config = https_config(&temp_dir, &server_cert, trusted_root)?;
            let (tx, rx) = mpsc::channel(1);
            let port = Self::spawn(config, tx).await?;

            Ok(TestFileTransferService {
                port,
                temp_dir,
                server_cert,
                server_err: rx,
            })
        }

        fn url_for(&self, path: &str) -> String {
            format!("https://localhost:{}/tedge/file-transfer/{path}", self.port)
        }

        /// An client with a client certificate that trusts the associated server certificate
        fn client_with_certificate(
            &self,
            cert: &rcgen::Certificate,
        ) -> anyhow::Result<reqwest::Client> {
            let mut pem = Vec::new();
            pem.extend(cert.serialize_private_key_pem().as_bytes());
            pem.extend(cert.serialize_pem().unwrap().as_bytes());
            let id = Identity::from_pem(&pem).unwrap();

            self.client_builder()?
                .identity(id)
                .build()
                .context("building client with identity")
        }

        /// An anonymous client that trusts the associated server certificate
        fn anonymous_client(&self) -> anyhow::Result<reqwest::Client> {
            self.client_builder()?
                .build()
                .context("building anonymous client")
        }

        #[allow(clippy::disallowed_types, clippy::disallowed_methods)]
        fn client_builder(&self) -> anyhow::Result<reqwest::ClientBuilder> {
            let reqwest_certificate = Certificate::from_der(
                &self
                    .server_cert
                    .serialize_der()
                    .context("serializing server certificate as der")?,
            )
            .context("building reqwest client")?;

            Ok(reqwest::Client::builder().add_root_certificate(reqwest_certificate))
        }
    }

    type TestConfig = FileTransferServerConfig<InjectedValue<String>, InjectedValue<RootCertStore>>;

    impl<Cert> TestFileTransferService<Cert> {
        fn temp_path_for(&self, file: &str) -> Utf8PathBuf {
            self.temp_dir.utf8_path().join("file-transfer").join(file)
        }

        async fn spawn(
            config: TestConfig,
            mut error_tx: Sender<RuntimeError>,
        ) -> anyhow::Result<u16> {
            let builder = FileTransferServerBuilder::try_bind(config).await?;
            let port = builder.listener.local_addr()?.port();
            let actor = builder.build();

            tokio::spawn(async move {
                if let Err(e) = actor.run().await {
                    let _ = error_tx.try_send(e);
                }
            });
            Ok(port)
        }
    }

    async fn create_listener() -> anyhow::Result<(TcpListener, u16)> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding to loopback port 0")?;
        let port = listener
            .local_addr()
            .context("retrieving local address for tcp listener")?
            .port();
        Ok((listener, port))
    }

    fn http_config(ttd: &TempTedgeDir, bind_port: u16) -> TestConfig {
        TestConfig {
            file_transfer_dir: DataDir::from(ttd.utf8_path_buf()).file_transfer_dir(),
            cert_path: OptionalConfig::empty("http.cert_path"),
            key_path: OptionalConfig::empty("http.key_path"),
            ca_path: OptionalConfig::empty("http.ca_path"),
            bind_addr: ([127, 0, 0, 1], bind_port).into(),
        }
    }

    fn https_config(
        ttd: &TempTedgeDir,
        server_cert: &rcgen::Certificate,
        trusted_root_cert: Option<&rcgen::Certificate>,
    ) -> anyhow::Result<TestConfig> {
        let cert = server_cert
            .serialize_pem()
            .context("serializing server certificate as pem")?;
        let key = server_cert.serialize_private_key_pem();

        let root_certs = if let Some(trusted_root) = trusted_root_cert {
            let mut store = RootCertStore::empty();
            store.add_parsable_certificates(&[trusted_root.serialize_der().unwrap()]);
            Some(store)
        } else {
            None
        };

        Ok(TestConfig {
            file_transfer_dir: DataDir::from(ttd.utf8_path_buf()).file_transfer_dir(),
            cert_path: OptionalConfig::present(InjectedValue(cert), "http.cert_path"),
            key_path: OptionalConfig::present(InjectedValue(key), "http.key_path"),
            ca_path: root_certs
                .map(|c| OptionalConfig::present(InjectedValue(c), "http.ca_path"))
                .unwrap_or_else(|| OptionalConfig::Empty("http.ca_path")),
            bind_addr: ([127, 0, 0, 1], 0).into(),
        })
    }
}
