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

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let http_config = self.config.clone();
        let server = http_file_transfer_server(&http_config)?;

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
    pub fn new(config: HttpConfig) -> Self {
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
    use hyper::Body;
    use hyper::Method;
    use hyper::Request;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::fs;

    #[tokio::test]
    async fn http_server_put_and_get() {
        let test_url = "http://127.0.0.1:4000/tedge/file-transfer/test-file";
        let ttd = TempTedgeDir::new();

        let http_config = HttpConfig::default()
            .with_data_dir(ttd.utf8_path_buf())
            .with_port(4000);

        // Spawn HTTP file transfer server
        let builder = FileTransferServerBuilder::new(http_config);
        let mut actor = builder.build();
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
    #[serial_test::serial]
    async fn check_server_does_not_panic_when_port_is_in_use() -> Result<(), anyhow::Error> {
        let ttd = TempTedgeDir::new();

        let http_config = HttpConfig::default()
            .with_data_dir(ttd.utf8_path_buf())
            .with_port(3746);
        let config_clone = http_config.clone();

        // Spawn HTTP file transfer server
        // handle_one uses port 3000.
        let builder_one = FileTransferServerBuilder::new(http_config);
        let handle_one = tokio::spawn(async move { builder_one.build().run().await });

        // handle_two will not be able to bind to the same port.
        let builder_two = FileTransferServerBuilder::new(config_clone);
        let handle_two = tokio::spawn(async move { builder_two.build().run().await });

        // although the code inside handle_two throws an error it does not panic.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // to check for the error, we assert that handle_one is still running
        // while handle_two is finished.
        assert!(!handle_one.is_finished());
        assert!(handle_two.is_finished());

        Ok(())
    }
}
