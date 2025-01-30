use std::convert::Infallible;

use crate::HttpError;
use crate::HttpRequest;
use crate::HttpResponse;
use crate::HttpResult;
use async_trait::async_trait;
use http::StatusCode;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Empty;
use http_body_util::Full;
use hyper::body::Body;
use hyper::body::Bytes;
use serde::de::DeserializeOwned;

/// Test helper aimed to decode HttpRequest
#[async_trait]
pub trait HttpRequestExt {
    /// Try to deserialize the request body as JSON.
    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError>;
}

#[async_trait]
impl HttpRequestExt for HttpRequest {
    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError> {
        let bytes = self.into_body().collect().await?.to_bytes();
        Ok(serde_json::from_slice(&bytes)?)
    }
}

/// An Http Response builder
pub struct HttpResponseBuilder {
    inner: http::response::Builder,
    body: Result<BoxBody<Bytes, hyper::Error>, HttpError>,
}

impl HttpResponseBuilder {
    /// Start to bo build a response
    pub fn new() -> Self {
        HttpResponseBuilder {
            inner: hyper::Response::builder(),
            body: Ok(Empty::new().map_err(|i| match i {}).boxed()),
        }
    }

    /// Set the status of the response
    ///
    /// If not set, the default is 200 OK.
    pub fn status<T>(self, status: T) -> Self
    where
        StatusCode: TryFrom<T>,
        <StatusCode as TryFrom<T>>::Error: Into<http::Error>,
    {
        HttpResponseBuilder {
            inner: self.inner.status(status),
            ..self
        }
    }

    /// Send a JSON body
    pub fn json<T: serde::Serialize + ?Sized>(self, json: &T) -> Self {
        let body = serde_json::to_vec(json)
            .map(|bytes| Full::new(bytes.into()).map_err(|i| match i {}).boxed())
            .map_err(|err| err.into());
        HttpResponseBuilder { body, ..self }
    }

    /// Send a  body
    pub fn body(
        self,
        content: impl Body<Data = Bytes, Error = Infallible> + Send + Sync + 'static,
    ) -> Self {
        let body = Ok(content.map_err(|i| match i {}).boxed());
        HttpResponseBuilder { body, ..self }
    }

    /// Build the response
    pub fn build(self) -> HttpResult {
        self.body.and_then(|body| {
            self.inner
                .body(body)
                .map(|body| HttpResponse {
                    response: body,
                    endpoint: "<test response>".to_string(),
                    method: "TEST".parse().unwrap(),
                })
                .map_err(|err| err.into())
        })
    }
}

impl Default for HttpResponseBuilder {
    fn default() -> Self {
        HttpResponseBuilder::new()
    }
}

/// Assert that some request is equal to the expected one.
pub fn assert_request_eq(actual: Option<HttpRequest>, expected: Option<HttpRequest>) {
    assert_eq!(actual.is_some(), expected.is_some());
    if let (Some(actual), Some(expected)) = (actual, expected) {
        assert_eq!(actual.method(), expected.method());
        assert_eq!(actual.uri(), expected.uri());
        assert_eq!(actual.headers(), expected.headers());
    }
}
