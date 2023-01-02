use http::header::HeaderName;
use http::header::HeaderValue;
use thiserror::Error;

#[derive(Clone, Debug, Default)]
pub struct HttpConfig {}

#[derive(Error, Debug)]
pub enum HttpParseError {
    #[error(transparent)]
    HttpError(#[from] http::Error),

    #[error(transparent)]
    JsonError(#[from] serde_json::Error),
}

pub type HttpError = hyper::Error;

pub type HttpRequest = http::Request<hyper::Body>;

pub type HttpResponse = http::Response<hyper::Body>;

pub type HttpResult = Result<HttpResponse, HttpError>;

/// An Http Request builder
pub struct HttpRequestBuilder {
    inner: http::request::Builder,
    body: Result<hyper::Body, HttpParseError>,
}

impl HttpRequestBuilder {
    /// Build the request
    pub fn build(self) -> Result<HttpRequest, HttpParseError> {
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

    /// Add bearer authentication (e.g. a JWT token)
    pub fn bearer_auth<T>(self, token: T) -> Self
    where
        T: std::fmt::Display,
    {
        let header_value = format!("Bearer {}", token);
        self.header(http::header::AUTHORIZATION, header_value)
    }
}
