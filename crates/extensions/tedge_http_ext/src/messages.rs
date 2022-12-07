pub use reqwest::Method;
pub use reqwest::StatusCode;
use reqwest::Url;
use thiserror::Error;

#[derive(Clone, Debug, Default)]
pub struct HttpConfig {}

#[derive(Error, Debug)]
pub enum HttpError {
    #[error(transparent)]
    HttpError(#[from] reqwest::Error),

    #[error(transparent)]
    ParseError(#[from] url::ParseError),
}

// It could be good to use directly `reqwest::Request` here.
// This would avoid to have boilerplate code and give more freedom to the callers.
// However, in the case of thin-edge one needs to add a bearer (a JWT token).
// and it's better to get this JWT token just before executing the request
// (caching the token in a serialized message has been proven to be problematic).
// An alternative could be then to encapsulate a `reqwest::RequestBuilder` instead.
#[derive(Debug)]
pub struct HttpRequest {
    pub method: Method,
    pub url: Url,
}

impl HttpRequest {
    pub fn new(method: Method, url: &str) -> Result<Self, HttpError> {
        let url = reqwest::Url::parse(url)?;
        Ok(HttpRequest { method, url })
    }
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: StatusCode,
}

impl From<HttpRequest> for reqwest::Request {
    fn from(request: HttpRequest) -> Self {
        reqwest::Request::new(request.method, request.url)
    }
}

impl From<reqwest::Response> for HttpResponse {
    fn from(response: reqwest::Response) -> Self {
        let status = response.status();
        HttpResponse { status }
    }
}
