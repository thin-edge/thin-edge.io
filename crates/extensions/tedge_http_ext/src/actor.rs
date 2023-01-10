use crate::HttpConfig;
use crate::HttpError;
use crate::HttpRequest;
use crate::HttpResult;
use async_trait::async_trait;
use hyper::client::Client;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use hyper_rustls::HttpsConnectorBuilder;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ConcurrentServiceMessageBox;
use tedge_actors::MessageBox;

pub(crate) struct HttpActor {
    client: Client<HttpsConnector<HttpConnector>, hyper::body::Body>,
}

impl HttpActor {
    pub(crate) fn new(_config: HttpConfig) -> Result<Self, HttpError> {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder().build(https);
        Ok(HttpActor { client })
    }
}

#[async_trait]
impl Actor for HttpActor {
    type MessageBox = ConcurrentServiceMessageBox<HttpRequest, HttpResult>;

    fn name(&self) -> &str {
        "HTTP"
    }

    async fn run(self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        while let Some((client_id, request)) = messages.recv().await {
            let client = self.client.clone();

            // Spawn the request
            let pending_result = tokio::spawn(async move {
                let response = client.request(request).await;
                (client_id, response)
            });

            // Send the response back to the client
            messages.send_response_once_done(pending_result)
        }

        Ok(())
    }
}
