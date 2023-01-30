use crate::HttpError;
use crate::HttpRequest;
use crate::HttpResult;
use http::StatusCode;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::ChannelError;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;

/// An Http Response builder
pub struct HttpResponseBuilder {
    inner: http::response::Builder,
    body: Result<hyper::Body, HttpError>,
}

impl HttpResponseBuilder {
    /// Start to bo build a response
    pub fn new() -> Self {
        HttpResponseBuilder {
            inner: hyper::Response::builder(),
            body: Ok(hyper::Body::empty()),
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
            .map(|bytes| bytes.into())
            .map_err(|err| err.into());
        HttpResponseBuilder { body, ..self }
    }

    /// Send a  body
    pub fn body(self, content: impl Into<hyper::Body>) -> Self {
        let body = Ok(content.into());
        HttpResponseBuilder { body, ..self }
    }

    /// Build the response
    pub fn build(self) -> HttpResult {
        self.body
            .and_then(|body| self.inner.body(body).map_err(|err| err.into()))
    }
}

/// A message box to mimic the behavior of an HTTP server.
///
/// Messages received by this box are those sent over HTTP to the real server.
/// Messages sent by this box mimic the response of the server.
///
/// This fake server panics on error.
pub struct FakeHttpServerBox {
    messages: SimpleMessageBox<HttpRequest, HttpResult>,
}

impl FakeHttpServerBox {
    /// Return a fake http message box builder
    pub fn builder() -> SimpleMessageBoxBuilder<HttpRequest, HttpResult> {
        SimpleMessageBoxBuilder::new("Fake Http Server", 16)
    }

    /// Receive a request
    pub async fn recv(&mut self) -> Option<HttpRequest> {
        self.messages.recv().await
    }

    /// Send a response
    pub async fn send(&mut self, response: HttpResult) -> Result<(), ChannelError> {
        self.messages.send(response).await
    }

    /// Assert that the next request is equal to some expected request
    pub async fn assert_recv(&mut self, expected: Option<HttpRequest>) {
        assert_request_eq(self.recv().await, expected);
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

impl Builder<FakeHttpServerBox> for SimpleMessageBoxBuilder<HttpRequest, HttpResult> {
    type Error = Infallible;

    fn try_build(self) -> Result<FakeHttpServerBox, Infallible> {
        Ok(FakeHttpServerBox {
            messages: self.build(),
        })
    }
}
