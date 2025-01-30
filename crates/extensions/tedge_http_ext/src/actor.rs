use crate::HttpRequest;
use crate::HttpResponse;
use crate::HttpResult;
use async_trait::async_trait;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt as _;
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rustls::ClientConfig;
use tedge_actors::Server;

#[derive(Clone)]
pub struct HttpService {
    client: Client<HttpsConnector<HttpConnector>, BoxBody<Bytes, hyper::Error>>,
}

impl HttpService {
    pub(crate) fn new(client_config: ClientConfig) -> Self {
        let https = HttpsConnectorBuilder::new()
            .with_tls_config(client_config)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(https);
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
        Ok(HttpResponse {
            endpoint: request.uri().path().to_owned(),
            method: request.method().to_owned(),
            response: self.client.request(request).await?.map(|b| b.boxed()),
        })
    }
}
