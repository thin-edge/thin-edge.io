use crate::HttpError;
use crate::HttpRequest;
use crate::HttpResponse;
use crate::HttpResult;
use async_trait::async_trait;
use backoff::ExponentialBackoff;
use http::request;
use http::HeaderValue;
use http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt as _;
use http_body_util::Empty;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rustls::ClientConfig;
use tedge_actors::Server;

use certificate::http_client::USER_AGENT;

#[derive(Clone)]
pub struct HttpService {
    client: Client<HttpsConnector<HttpConnector>, BoxBody<Bytes, hyper::Error>>,
    backoff: ExponentialBackoff,
}

impl HttpService {
    pub(crate) fn new(client_config: ClientConfig, backoff: ExponentialBackoff) -> Self {
        let https = HttpsConnectorBuilder::new()
            .with_tls_config(client_config)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(https);
        HttpService { client, backoff }
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
        let mut request = request;
        request
            .request
            .headers_mut()
            .entry(http::header::USER_AGENT)
            .or_insert_with(|| HeaderValue::from_static(USER_AGENT));

        // the dance below happens because HttpRequest is not clone, because BoxedBody is not clone,
        // so we would not be able to clone the request to retry it.
        //
        // To work around that, we change actor request type to a request which is clonable, and
        // then clone the body before wrapping it into a BoxedBody.
        let allowed_retry_statuses = request.retry_statuses;
        let endpoint = request.request.uri().path().to_owned();
        let method = request.request.method().to_owned();
        let (parts, body) = request.request.into_parts();

        let backoff = self.backoff.clone();
        let operation = || {
            let body = match body.as_ref() {
                Some(body) => Full::new(body.clone()).map_err(|i| match i {}).boxed(),
                None => Empty::new().map_err(|i| match i {}).boxed(),
            };
            let request = request::Request::from_parts(parts.clone(), body);
            let endpoint = endpoint.clone();
            let method = method.clone();

            async {
                let response = self.client.request(request).await;
                match response {
                    Err(err) => Err(to_backoff_error(err)),
                    Ok(response)
                        if response.status().is_client_error()
                            || response.status().is_server_error() =>
                    {
                        if is_status_retryable(response.status(), &allowed_retry_statuses) {
                            Err(backoff::Error::transient(HttpError::HttpStatusError {
                                code: response.status(),
                                endpoint,
                                method,
                            }))
                        } else {
                            Err(backoff::Error::permanent(HttpError::HttpStatusError {
                                code: response.status(),
                                endpoint,
                                method,
                            }))
                        }
                    }
                    Ok(response) => Ok(response),
                }
            }
        };
        let response = match &method {
            m if m.is_idempotent() => backoff::future::retry(backoff, operation).await,
            _ => operation().await.map_err(|err| match err {
                backoff::Error::Permanent(err) | backoff::Error::Transient { err, .. } => err,
            }),
        };
        let response = response?;

        Ok(HttpResponse {
            endpoint,
            method,
            response: response.map(|b| b.boxed()),
        })
    }
}

fn to_backoff_error(err: hyper_util::client::legacy::Error) -> backoff::Error<HttpError> {
    if err.is_connect() {
        backoff::Error::transient(HttpError::HyperUtilError(err))
    } else {
        backoff::Error::permanent(HttpError::HyperUtilError(err))
    }
}

fn is_status_retryable(status: StatusCode, allowed_retry_statuses: &[StatusCode]) -> bool {
    allowed_retry_statuses.contains(&status)
        || certificate::http_client::is_status_retryable(status)
}
