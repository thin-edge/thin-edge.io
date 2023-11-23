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
pub(crate) struct FileTransferServerConfig<ConfigPath, CaPath> {
    pub file_transfer_dir: Utf8PathBuf,
    pub cert_path: OptionalConfig<ConfigPath>,
    pub key_path: OptionalConfig<ConfigPath>,
    pub ca_path: CaPath,
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
        socket_addr: SocketAddr,
        config: FileTransferServerConfig<impl PemReader, impl TrustStoreLoader>,
    ) -> Result<Self, anyhow::Error> {
        let listener = TcpListener::bind(socket_addr)
            .await
            .with_context(|| format!("Binding file-transfer server to {socket_addr}"))?;
        Self::try_new(listener, config)
    }

    pub(crate) fn try_new(
        listener: TcpListener,
        cfg: FileTransferServerConfig<impl PemReader, impl TrustStoreLoader>,
    ) -> anyhow::Result<Self> {
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        Ok(Self {
            rustls_config: load_ssl_config(cfg.cert_path, cfg.key_path, cfg.ca_path)?,
            file_transfer_dir: cfg.file_transfer_dir,
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
    use tokio::task::JoinHandle;

    #[tokio::test]
    async fn http_server_put_and_get() -> anyhow::Result<()> {
        let server = Server::new_http().await?;
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

        let binding_res = FileTransferServerBuilder::try_bind(
            ([127, 0, 0, 1], port_in_use).into(),
            http_config(&ttd),
        )
        .await;

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
        let server = Server::new_https(server_cert, None).await?;
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
        let server = Server::new_https(server_cert, Some(&client_cert)).await?;
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
        let server = Server::new_https(server_cert, Some(&client_cert)).await?;

        let client = server.anonymous_client()?;
        let test_url = server.url_for("test/file");

        let upload_err = client.put(&test_url).body("file").send().await.unwrap_err();
        axum_tls::assert_error_matches(upload_err, rustls::AlertDescription::CertificateRequired);

        Ok(())
    }

    struct Server<Cert> {
        port: u16,
        temp_dir: TempTedgeDir,
        server_cert: Cert,
        server_err: Receiver<RuntimeError>,
    }

    impl Server<()> {
        async fn new_http() -> anyhow::Result<Self> {
            let (listener, port) = create_listener().await?;
            let temp_dir = TempTedgeDir::new();
            let config = http_config(&temp_dir);
            let (tx, rx) = mpsc::channel(1);
            Self::spawn(listener, config, tx)?;

            Ok(Server {
                port,
                temp_dir,
                server_cert: (),
                server_err: rx,
            })
        }

        fn url_for(&self, path: &str) -> String {
            format!("http://localhost:{}/tedge/file-transfer/{path}", self.port)
        }

        fn client(&self) -> reqwest::Client {
            reqwest::Client::new()
        }
    }

    impl<C> Drop for Server<C> {
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

    impl Server<rcgen::Certificate> {
        async fn new_https(
            server_cert: rcgen::Certificate,
            trusted_root: Option<&rcgen::Certificate>,
        ) -> anyhow::Result<Self> {
            let (listener, port) = create_listener().await?;
            let temp_dir = TempTedgeDir::new();
            let config = https_config(&temp_dir, &server_cert, trusted_root)?;
            let (tx, rx) = mpsc::channel(1);
            Self::spawn(listener, config, tx)?;

            Ok(Server {
                port,
                temp_dir,
                server_cert,
                server_err: rx,
            })
        }

        fn url_for(&self, path: &str) -> String {
            format!("https://localhost:{}/tedge/file-transfer/{path}", self.port)
        }

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

        fn anonymous_client(&self) -> anyhow::Result<reqwest::Client> {
            self.client_builder()?
                .build()
                .context("building anonymous client")
        }

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

    type TestConfig =
        FileTransferServerConfig<InjectedValue<String>, InjectedValue<Option<RootCertStore>>>;

    impl<Cert> Server<Cert> {
        fn temp_path_for(&self, file: &str) -> Utf8PathBuf {
            self.temp_dir.utf8_path().join("file-transfer").join(file)
        }

        fn spawn(
            listener: TcpListener,
            config: TestConfig,
            mut error_tx: Sender<RuntimeError>,
        ) -> anyhow::Result<JoinHandle<()>> {
            let builder = FileTransferServerBuilder::try_new(listener, config)?;
            let actor = builder.build();
            Ok(tokio::spawn(async move {
                if let Err(e) = actor.run().await {
                    let _ = error_tx.try_send(e);
                }
            }))
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

    fn http_config(ttd: &TempTedgeDir) -> TestConfig {
        TestConfig {
            file_transfer_dir: DataDir::from(ttd.utf8_path_buf()).file_transfer_dir(),
            cert_path: OptionalConfig::empty("http.cert_path"),
            key_path: OptionalConfig::empty("http.key_path"),
            ca_path: InjectedValue(None),
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
            ca_path: InjectedValue(root_certs),
        })
    }
}
