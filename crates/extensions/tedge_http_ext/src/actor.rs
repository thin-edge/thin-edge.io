use crate::HttpRequest;
use crate::HttpResult;
use async_trait::async_trait;
use hyper::client::Client;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use hyper_rustls::HttpsConnectorBuilder;
use tedge_actors::Server;

#[derive(Clone)]
pub struct HttpService {
    client: Client<HttpsConnector<HttpConnector>, hyper::body::Body>,
}

impl HttpService {
    pub(crate) fn new() -> Self {
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder().build(https);
        HttpService { client }
    }
}

#[async_trait]
impl Server for HttpService {
    type Request = HttpRequest;
    type Response = HttpResult;

    fn name(&self) -> &str {
        "HTTP"
    }

    async fn handle(&mut self, request: Self::Request) -> Self::Response {
        Ok(self.client.request(request).await?)
    }
}
