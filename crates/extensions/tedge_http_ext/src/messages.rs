use async_trait::async_trait;
use http::header::HeaderName;
use http::header::HeaderValue;
use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Clone, Debug, Default)]
pub struct HttpConfig {}

#[derive(Error, Debug)]
pub enum HttpError {
    #[error(transparent)]
    HttpError(#[from] http::Error),

    #[error("Failed with HTTP error status {0}")]
    HttpStatusError(http::status::StatusCode),

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),
}

pub type HttpRequest = http::Request<hyper::Body>;

pub type HttpResponse = http::Response<hyper::Body>;

pub type HttpResult = Result<HttpResponse, HttpError>;

pub type HttpBytes = hyper::body::Bytes;

/// An Http Request builder
pub struct HttpRequestBuilder {
    inner: http::request::Builder,
    body: Result<hyper::Body, HttpError>,
}

impl HttpRequestBuilder {
    /// Build the request
    pub fn build(self) -> Result<HttpRequest, HttpError> {
        self.body
            .and_then(|body| self.inner.body(body).map_err(|err| err.into()))
    }

    /// Start to build a GET request
    pub fn get<T>(uri: T) -> Self
    where
        hyper::Uri: TryFrom<T>,
        <hyper::Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        HttpRequestBuilder {
            inner: hyper::Request::get(uri),
            body: Ok(hyper::Body::empty()),
        }
    }

    /// Start to build a POST request
    pub fn post<T>(uri: T) -> Self
    where
        hyper::Uri: TryFrom<T>,
        <hyper::Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        HttpRequestBuilder {
            inner: hyper::Request::post(uri),
            body: Ok(hyper::Body::empty()),
        }
    }

    /// Add an HTTP header to this request
    pub fn header<K, V>(self, key: K, value: V) -> Self
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        HttpRequestBuilder {
            inner: self.inner.header(key, value),
            ..self
        }
    }

    /// Send a JSON body
    pub fn json<T: serde::Serialize + ?Sized>(self, json: &T) -> Self {
        let body = serde_json::to_vec(json)
            .map(|bytes| bytes.into())
            .map_err(|err| err.into());
        HttpRequestBuilder { body, ..self }
    }

    /// Send a  body
    pub fn body(self, content: impl Into<hyper::Body>) -> Self {
        let body = Ok(content.into());
        HttpRequestBuilder { body, ..self }
    }

    /// Add bearer authentication (e.g. a JWT token)
    pub fn bearer_auth<T>(self, token: T) -> Self
    where
        T: std::fmt::Display,
    {
        let header_value = format!("Bearer {}", token);
        self.header(http::header::AUTHORIZATION, header_value)
    }
}

#[async_trait]
pub trait HttpResponseExt {
    /// Turn a response into an error if the server returned an error.
    fn error_for_status(self) -> HttpResult;

    /// Get the full response body as Bytes.
    async fn bytes(self) -> Result<HttpBytes, HttpError>;

    /// Get the full response body as Bytes.
    async fn text(self) -> Result<String, HttpError>;

    /// Try to deserialize the response body as JSON.
    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError>;
}

#[async_trait]
impl HttpResponseExt for HttpResponse {
    fn error_for_status(self) -> HttpResult {
        let status = self.status();
        if status.is_success() {
            Ok(self)
        } else {
            Err(HttpError::HttpStatusError(status))
        }
    }

    async fn bytes(self) -> Result<HttpBytes, HttpError> {
        Ok(hyper::body::to_bytes(self.into_body()).await?)
    }

    async fn text(self) -> Result<String, HttpError> {
        let bytes = self.bytes().await?;
        Ok(String::from_utf8(bytes.to_vec())?)
    }

    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError> {
        let bytes = self.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

#[async_trait]
impl HttpResponseExt for HttpResult {
    fn error_for_status(self) -> HttpResult {
        match self {
            Ok(response) => response.error_for_status(),
            Err(err) => Err(err),
        }
    }

    async fn bytes(self) -> Result<HttpBytes, HttpError> {
        match self {
            Ok(response) => response.bytes().await,
            Err(err) => Err(err),
        }
    }

    async fn text(self) -> Result<String, HttpError> {
        match self {
            Ok(response) => response.text().await,
            Err(err) => Err(err),
        }
    }

    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError> {
        match self {
            Ok(response) => response.json().await,
            Err(err) => Err(err),
        }
    }
}
