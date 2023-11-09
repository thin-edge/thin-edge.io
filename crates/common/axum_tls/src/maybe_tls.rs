use pin_project::pin_project;
use std::io;
use std::io::Error;
use std::pin::Pin;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::BufReader;
use tokio::io::ReadBuf;
use tokio_rustls::server::TlsStream;

#[pin_project(project = MaybeTlsStreamProj)]
pub enum MaybeTlsStream<I> {
    Tls(#[pin] Box<TlsStream<BufReader<I>>>),
    Insecure(#[pin] BufReader<I>),
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncRead for MaybeTlsStream<I> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeTlsStreamProj::Tls(tls) => tls.poll_read(cx, buf),
            MaybeTlsStreamProj::Insecure(insecure) => insecure.poll_read(cx, buf),
        }
    }
}

impl<I: AsyncRead + AsyncWrite + Unpin> AsyncWrite for MaybeTlsStream<I> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.project() {
            MaybeTlsStreamProj::Tls(tls) => tls.poll_write(cx, buf),
            MaybeTlsStreamProj::Insecure(insecure) => insecure.poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        match self.project() {
            MaybeTlsStreamProj::Tls(tls) => tls.poll_flush(cx),
            MaybeTlsStreamProj::Insecure(insecure) => insecure.poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<(), Error>> {
        match self.project() {
            MaybeTlsStreamProj::Tls(tls) => tls.poll_shutdown(cx),
            MaybeTlsStreamProj::Insecure(insecure) => insecure.poll_shutdown(cx),
        }
    }
}
