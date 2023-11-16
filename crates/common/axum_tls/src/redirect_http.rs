use crate::acceptor::Acceptor;
use crate::acceptor::TlsData;
use axum::http::uri::Authority;
use axum::http::uri::InvalidUriParts;
use axum::http::uri::Scheme;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use tracing::error;

pub async fn redirect_http_to_https<B>(
    request: Request<B>,
) -> Result<Request<B>, HttpsRedirectResponse> {
    let tls_data = request
        .extensions()
        .get::<TlsData>()
        .ok_or(HttpsRedirectResponse::MissingTlsData)?;

    if tls_data.is_secure {
        Ok(request)
    } else {
        let host = request
            .headers()
            .get("host")
            .ok_or(HttpsRedirectResponse::MissingHostHeader)?
            .to_owned();
        let mut uri = request.uri().to_owned().into_parts();
        uri.scheme = Some(Scheme::HTTPS);
        uri.authority = Some(Authority::from_maybe_shared(host).unwrap());
        let uri = Uri::try_from(uri).map_err(HttpsRedirectResponse::FailedToCreateUri)?;
        Err(HttpsRedirectResponse::RedirectTo(uri.to_string()))
    }
}

#[derive(Debug)]
pub enum HttpsRedirectResponse {
    MissingTlsData,
    MissingHostHeader,
    FailedToCreateUri(InvalidUriParts),
    RedirectTo(String),
}

impl IntoResponse for HttpsRedirectResponse {
    fn into_response(self) -> Response {
        let internal_err = (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal error in tedge-mapper",
        );
        match self {
            Self::MissingTlsData => {
                error!(
                    "{} not set. Are you using the right acceptor ({})?",
                    std::any::type_name::<TlsData>(),
                    std::any::type_name::<Acceptor>()
                );
                internal_err.into_response()
            }
            Self::FailedToCreateUri(e) => {
                error!("{}", anyhow::Error::new(e).context("Failed to create URI"));
                internal_err.into_response()
            }
            Self::MissingHostHeader => (
                StatusCode::BAD_REQUEST,
                "This server does not support HTTP. Please retry this request using HTTPS",
            )
                .into_response(),
            Self::RedirectTo(uri) => Redirect::temporary(&uri).into_response(),
        }
        .into_response()
    }
}
