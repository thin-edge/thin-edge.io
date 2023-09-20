use std::future::ready;

use axum::async_trait;
use axum::extract::FromRequest;
use futures::stream::once;
use futures::StreamExt;
use hyper::body::Bytes;
use hyper::Body;
use hyper::Request;
use hyper::StatusCode;

pub enum PossiblySmallBody {
    Small(Bytes),
    Large(Body),
}

impl PossiblySmallBody {
    pub fn try_clone(self) -> (Body, Option<Bytes>) {
        match self {
            Self::Small(bytes) => (bytes.clone().into(), Some(bytes)),
            Self::Large(body) => (body, None),
        }
    }
}

#[async_trait]
impl<S> FromRequest<S, Body> for PossiblySmallBody
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request(mut req: Request<Body>, _: &S) -> Result<Self, Self::Rejection> {
        if let Some(length) = req.headers().get("content-length") {
            // TODO no unwrap
            let length = length
                .to_str()
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Content-Length header is not valid ASCII",
                    )
                })?
                .parse::<usize>()
                .map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        "Content-Length header is not a number",
                    )
                })?;
            if length <= 1024 * 1024 {
                let mut bytes = Vec::with_capacity(length);
                while let Some(chunk) = req.body_mut().next().await {
                    let chunk = chunk.unwrap();
                    if chunk.len() <= bytes.capacity() - bytes.len() {
                        bytes.append(&mut chunk.into())
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            "Body exceeds the specified Content-Length",
                        ));
                    }
                }
                return Ok(PossiblySmallBody::Small(bytes.into()));
            }
        }

        let mut body = std::mem::take(req.body_mut());
        Ok(if let Some(result) = body.next().await {
            PossiblySmallBody::Large(Body::wrap_stream(once(ready(result)).chain(body)))
        } else {
            PossiblySmallBody::Small(Bytes::new())
        })
    }
}
