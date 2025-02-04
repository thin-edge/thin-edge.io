use std::convert::Infallible;

use async_trait::async_trait;
use http::header::HeaderName;
use http::header::HeaderValue;
use http::HeaderMap;
use http::Method;
use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use http_body_util::Empty;
use http_body_util::Full;
use hyper::body::Bytes;
use serde::de::DeserializeOwned;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum HttpError {
    #[error(transparent)]
    HttpError(#[from] http::Error),

    #[error("Failed with HTTP error status {code} for {method} request to endpoint {endpoint}")]
    HttpStatusError {
        code: http::status::StatusCode,
        endpoint: String,
        method: Method,
    },

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    HyperError(#[from] hyper::Error),

    #[error(transparent)]
    HyperUtilError(#[from] hyper_util::client::legacy::Error),
}

type Body = BoxBody<Bytes, hyper::Error>;
pub type HttpRequest = http::Request<Body>;

#[derive(Debug)]
pub struct HttpResponse {
    pub response: http::Response<Body>,
    pub endpoint: String,
    pub method: Method,
}

impl HttpResponse {
    pub fn status(&self) -> http::StatusCode {
        self.response.status()
    }
}

pub type HttpResult = Result<HttpResponse, HttpError>;

pub type HttpBytes = hyper::body::Bytes;

/// An Http Request builder
pub struct HttpRequestBuilder {
    inner: http::request::Builder,
    body: Result<Body, HttpError>,
}

fn infallible<T>(i: Infallible) -> T {
    match i {}
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
            body: Ok(Empty::new().map_err(infallible).boxed()),
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
            body: Ok(Empty::new().map_err(infallible).boxed()),
        }
    }

    /// Start to build a PUT request
    pub fn put<T>(uri: T) -> Self
    where
        hyper::Uri: TryFrom<T>,
        <hyper::Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        HttpRequestBuilder {
            inner: hyper::Request::put(uri),
            body: Ok(Empty::new().map_err(infallible).boxed()),
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

    /// Add multiple headers at once
    pub fn headers(mut self, header_map: &HeaderMap) -> Self {
        let request = self.inner.headers_mut().unwrap();
        for (key, value) in header_map {
            request.insert(key, value.clone());
        }
        self
    }

    /// Send a JSON body
    pub fn json<T: serde::Serialize + ?Sized>(self, json: &T) -> Self {
        let body = serde_json::to_vec(json)
            .map(|bytes| Full::new(Bytes::from(bytes)).map_err(infallible).boxed())
            .map_err(|err| err.into());
        HttpRequestBuilder { body, ..self }
    }

    /// Send a  body
    pub fn body(self, content: impl Into<Body>) -> Self {
        let body = Ok(content.into());
        HttpRequestBuilder { body, ..self }
    }
}

#[async_trait]
pub trait HttpResponseExt {
    /// Turn a response into an error if the server returned an error.
    fn error_for_status(self) -> HttpResult;

    /// Get the full response body as Bytes.
    async fn bytes(self) -> Result<HttpBytes, HttpError>;

    /// Get the full response body as String.
    async fn text(self) -> Result<String, HttpError>;

    /// Try to deserialize the response body as JSON.
    async fn json<T: DeserializeOwned>(self) -> Result<T, HttpError>;
}

#[async_trait]
impl HttpResponseExt for HttpResponse {
    fn error_for_status(self) -> HttpResult {
        let status = self.response.status();
        if status.is_success() {
            Ok(self)
        } else {
            Err(HttpError::HttpStatusError {
                code: status,
                endpoint: self.endpoint,
                method: self.method,
            })
        }
    }

    async fn bytes(self) -> Result<HttpBytes, HttpError> {
        Ok(self.response.into_body().collect().await?.to_bytes())
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
