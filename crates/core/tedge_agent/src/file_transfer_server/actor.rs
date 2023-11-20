use crate::file_transfer_server::error::FileTransferError;
use crate::file_transfer_server::http_rest::http_file_transfer_server;
use crate::file_transfer_server::http_rest::HttpConfig;
use async_trait::async_trait;
use std::convert::Infallible;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tracing::log::info;

pub struct FileTransferServerActor {
    config: HttpConfig,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

/// HTTP file transfer server is stand-alone.
#[async_trait]
impl Actor for FileTransferServerActor {
    fn name(&self) -> &str {
        "HttpFileTransferServer"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        let http_config = self.config.clone();
        let server = http_file_transfer_server(http_config)?;

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
}

impl FileTransferServerBuilder {
    pub(crate) fn new(config: HttpConfig) -> Self {
        let (signal_sender, signal_receiver) = mpsc::channel(10);
        Self {
            config,
            signal_sender,
            signal_receiver,
        }
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use anyhow::ensure;
    use hyper::Body;
    use hyper::Method;
    use hyper::Request;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tedge_api::path::DataDir;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::fs;

    #[tokio::test]
    async fn http_server_put_and_get() {
        let test_url = "http://127.0.0.1:4000/tedge/file-transfer/test-file";
        let ttd = TempTedgeDir::new();

        // TODO make this port dynamic
        let http_config = http_config(&ttd, 4000);

        // Spawn HTTP file transfer server
        let builder = FileTransferServerBuilder::new(http_config);
        let actor = builder.build();
        tokio::spawn(async move { actor.run().await });

        // Create PUT request to file transfer service
        let put_handler = tokio::spawn(async move {
            let client = hyper::Client::new();

            let req = Request::builder()
                .method(Method::PUT)
                .uri(test_url)
                .body(Body::from(String::from("file")))
                .expect("request builder");
            client.request(req).await.unwrap()
        });

        let put_response = put_handler.await.unwrap();
        assert_eq!(put_response.status(), hyper::StatusCode::CREATED);

        // Check if a file is created.
        let file_path = ttd.path().join("file-transfer").join("test-file");
        assert!(file_path.exists());
        let file_content = fs::read_to_string(file_path).await.unwrap();
        assert_eq!(file_content, "file".to_string());

        // Create GET request to file transfer service
        let get_handler = tokio::spawn(async move {
            let client = hyper::Client::new();

            let req = Request::builder()
                .method(Method::GET)
                .uri(test_url)
                .body(Body::empty())
                .expect("request builder");
            client.request(req).await.unwrap()
        });

        let get_response = get_handler.await.unwrap();
        assert_eq!(get_response.status(), hyper::StatusCode::OK);
    }

    #[tokio::test]
    async fn check_server_does_not_panic_when_port_is_in_use() -> anyhow::Result<()> {
        let ttd = TempTedgeDir::new();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port_in_use = listener.local_addr().unwrap().port();

        let http_config = http_config(&ttd, port_in_use);

        let server = FileTransferServerBuilder::new(http_config).build().run();

        tokio::select! {
            res = server => ensure!(res.is_err(), "expected server startup to fail with port binding error, but actor exited successfully"),
            _ = tokio::time::sleep(Duration::from_secs(5)) => bail!("timed out waiting for actor to stop running"),
        }

        Ok(())
    }

    fn http_config(ttd: &TempTedgeDir, port: u16) -> HttpConfig {
        HttpConfig {
            file_transfer_dir: DataDir::from(ttd.utf8_path_buf()).file_transfer_dir(),
            bind_address: SocketAddr::from(([127, 0, 0, 1], port)),
            certificates: None,
        }
    }
}
