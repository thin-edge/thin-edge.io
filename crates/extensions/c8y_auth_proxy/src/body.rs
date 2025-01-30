use std::future::ready;

use axum::body::Body;
use axum::extract::FromRequest;
use futures::stream::once;
use futures::StreamExt;
use http_body::Frame;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Buf;
use hyper::body::Bytes;
use hyper::Request;
use hyper::StatusCode;
use tokio::sync::mpsc;

pub enum PossiblySmallBody {
    Small(Bytes),
    Large(SyncBody<Bytes, axum::Error>),
}

/// An adapter for [`http_body::Body`] implementations to guarantee [`Sync`]
///
/// Because [`reqwest::Body`] is `!Sync` and we need to convert it to
/// [`axum::body::Body`] which requires `Sync`, we need a wrapper type.
///
/// The wrapper spawns an async task which sends the resulting frames
/// to a [`mpsc::channel`]. This then implements `Sync`.
#[pin_project::pin_project]
pub struct SyncBody<D, E>(mpsc::Receiver<Result<Frame<D>, E>>);

impl<D, E> SyncBody<D, E>
where
    D: Send + 'static,
    E: Send + 'static,
{
    pub fn new(b: impl http_body::Body<Data = D, Error = E> + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel(10);
        tokio::spawn(async move {
            tokio::pin!(b);
            while let Some(frame) = b.frame().await {
                // unwrap because we should abort this loop if the receiver is dropped
                tx.send(frame).await.unwrap();
            }
        });
        Self(rx)
    }
}

impl<D: Buf, E> http_body::Body for SyncBody<D, E> {
    type Data = D;
    type Error = E;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let this = self.project();
        this.0.poll_recv(cx)
    }
}

impl PossiblySmallBody {
    pub fn try_clone(self) -> (SyncBody<Bytes, axum::Error>, Option<Bytes>) {
        match self {
            Self::Small(bytes) => (
                SyncBody::new(Full::new(bytes.clone()).map_err(|i| match i {})),
                Some(bytes),
            ),
            Self::Large(body) => (body, None),
        }
    }
}

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
                let mut stream = req.into_body().into_data_stream();
                while let Some(chunk) = stream.next().await {
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

        let body = std::mem::take(req.body_mut());
        let mut stream = body.into_data_stream();
        Ok(if let Some(result) = stream.next().await {
            PossiblySmallBody::Large(SyncBody::new(Body::from_stream(
                once(ready(result)).chain(stream),
            )))
        } else {
            PossiblySmallBody::Small(Bytes::new())
        })
    }
}
