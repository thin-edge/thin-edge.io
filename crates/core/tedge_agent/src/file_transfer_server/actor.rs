use crate::file_transfer_server::error::FileTransferError;
use crate::file_transfer_server::http_rest::http_file_transfer_server;
use crate::file_transfer_server::http_rest::HttpConfig;
use anyhow::Context;
use async_trait::async_trait;
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
use tokio::net::TcpListener;
use tracing::log::info;

pub struct FileTransferServerActor {
    config: HttpConfig,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    listener: TcpListener,
}

/// HTTP file transfer server is stand-alone.
#[async_trait]
impl Actor for FileTransferServerActor {
    fn name(&self) -> &str {
        "HttpFileTransferServer"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let server = http_file_transfer_server(self.listener, self.config)?;

        tokio::select! {
            result = server => {
                info!("Done");
                return Ok(result.map_err(FileTransferError::FromHyperError)?);
            }
            Some(RuntimeRequest::Shutdown) = self.signal_receiver.next() => {
                info!("Shutdown");
                return Ok(());
            }
        }
    }
}

pub struct FileTransferServerBuilder {
    config: HttpConfig,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
    listener: TcpListener,
}

impl FileTransferServerBuilder {
    pub(crate) async fn try_bind(
        config: HttpConfig,
        socket_addr: impl Into<SocketAddr>,
    ) -> Result<Self, anyhow::Error> {
        let addr = socket_addr.into();
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("Binding file-transfer server to {addr}"))?;
        Self::new(config, listener)
    }

    pub(crate) fn new(config: HttpConfig, listener: TcpListener) -> Result<Self, anyhow::Error> {
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        Ok(Self {
            config,
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
        Ok(self.build())
    }

    fn build(self) -> FileTransferServerActor {
        FileTransferServerActor {
            config: self.config,
            signal_receiver: self.signal_receiver,
            listener: self.listener,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::ensure;
    use tedge_api::path::DataDir;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::fs;

    #[tokio::test]
    async fn http_server_put_and_get() {
        let ttd = TempTedgeDir::new();
        let (listener, port) = create_listener().await;
        let test_url = format!("http://127.0.0.1:{port}/tedge/file-transfer/test-file");

        // Spawn HTTP file transfer server
        let builder = FileTransferServerBuilder::new(http_config(&ttd), listener).unwrap();
        let actor = builder.build();
        tokio::spawn(async move { actor.run().await });

        let client = reqwest::Client::new();

        let upload_response = client.put(&test_url).body("file").send().await.unwrap();
        assert_eq!(upload_response.status(), hyper::StatusCode::CREATED);

        // Check if a file is created.
        let file_path = ttd.path().join("file-transfer").join("test-file");
        assert!(file_path.exists());
        let file_content = fs::read_to_string(file_path).await.unwrap();
        assert_eq!(file_content, "file".to_string());

        let get_response = client.get(&test_url).send().await.unwrap();
        assert_eq!(get_response.status(), hyper::StatusCode::OK);
    }

    #[tokio::test]
    async fn check_server_does_not_panic_when_port_is_in_use() -> anyhow::Result<()> {
        let ttd = TempTedgeDir::new();

        let (_listener, port_in_use) = create_listener().await;

        let http_config = http_config(&ttd);

        let binding_res =
            FileTransferServerBuilder::try_bind(http_config, ([127, 0, 0, 1], port_in_use)).await;

        ensure!(
            binding_res.is_err(),
            "expected port binding to fail, but `try_bind` finished successfully"
        );

        Ok(())
    }

    async fn create_listener() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    fn http_config(ttd: &TempTedgeDir) -> HttpConfig {
        HttpConfig {
            file_transfer_dir: DataDir::from(ttd.utf8_path_buf()).file_transfer_dir(),
            certificates: None,
        }
    }
}
