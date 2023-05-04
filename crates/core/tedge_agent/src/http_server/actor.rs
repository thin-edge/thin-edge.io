use crate::http_server::http_rest::http_file_transfer_server;
use crate::http_server::http_rest::HttpConfig;
use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::NoMessage;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::SimpleMessageBoxBuilder;
use tracing::log::error;

pub struct HttpServerActor {
    config: HttpConfig,
}

impl HttpServerActor {
    pub fn new(config: HttpConfig) -> Self {
        HttpServerActor { config }
    }
}

/// HTTP file transfer server is stand-alone, therefore, no input or output.
#[async_trait]
impl Actor for HttpServerActor {
    fn name(&self) -> &str {
        "HttpFileTransferServer"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        let server = http_file_transfer_server(&self.config);

        match server {
            Ok(server) => {
                if let Err(err) = server.await {
                    error!("{}", err);
                }
            }
            Err(err) => error!("{}", err),
        }
        Ok(())
    }
}

pub struct HttpServerBuilder {
    config: HttpConfig,
    box_builder: SimpleMessageBoxBuilder<NoMessage, NoMessage>,
}

impl HttpServerBuilder {
    pub fn new(config: HttpConfig) -> Self {
        Self {
            config,
            box_builder: SimpleMessageBoxBuilder::new("HttpFileTransferServer", 4),
        }
    }
}

impl RuntimeRequestSink for HttpServerBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<HttpServerActor> for HttpServerBuilder {
    type Error = RuntimeError;

    fn try_build(self) -> Result<HttpServerActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> HttpServerActor {
        HttpServerActor::new(self.config)
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

    // HELP ME HERE: This test fails with multi thread due to connection refused. Why?
    // #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[tokio::test]
    async fn http_server_put_and_get() {
        let test_url = "http://127.0.0.1:4000/tedge/file-transfer/test-file";
        let ttd = TempTedgeDir::new();

        let http_config = HttpConfig::default()
            .with_data_dir(ttd.utf8_path_buf())
            .with_port(4000);

        // Spawn HTTP file transfer server
        let builder = HttpServerBuilder::new(http_config);
        let mut actor = builder.build();
        tokio::spawn(async move { actor.run().await });

        // Create PUT request to file transfer service
        let put_handler = tokio::spawn(async move {
            let client = hyper::Client::new();

            let req = Request::builder()
                .method(Method::PUT)
                .uri(test_url)
                .body(Body::from(String::from("file").clone()))
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
}
