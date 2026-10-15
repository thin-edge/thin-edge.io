//! Multiplexing mode: carries many TCP connections to the configured target over a single remote
//! access websocket using yamux (<https://github.com/hashicorp/yamux/blob/master/spec.md>), instead
//! of one websocket (and one remote access operation) per connection.
//!
//! The mode is negotiated in-band with multistream-select
//! (<https://github.com/multiformats/multistream-select>), so it works without changes to
//! Cumulocity and stays compatible in both directions:
//! - the client starts the session with the multistream-select header ([`MULTISTREAM_HEADER`]) and
//!   proposes [`YAMUX_PROTOCOL`]; the plugin confirms it and starts a yamux session (the plugin is
//!   the yamux server). Other protocols are declined, so the client can fall back.
//! - any other first data from the client, or data sent first by the target (e.g. SSH or VNC
//!   banners), selects the normal passthrough mode, with no data lost or added latency
//! - an older plugin forwards the header to the target, and the client falls back to one
//!   websocket per connection when the protocol is not confirmed
//!
//! The client cannot choose a destination: every stream is connected to the target of the remote
//! access configuration, so multiplexing does not widen what a session can reach.

use async_compat::CompatExt;
use futures_util::io::AsyncRead;
use futures_util::io::AsyncReadExt;
use futures_util::io::AsyncWrite;
use futures_util::io::AsyncWriteExt;
use std::io;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use tokio::net::TcpStream;

/// The multistream-select header (length-prefixed), sent first by a client requesting multiplexing
pub const MULTISTREAM_HEADER: &[u8] = b"\x13/multistream/1.0.0\n";
/// The protocol of a multiplexed session
pub const YAMUX_PROTOCOL: &str = "/yamux/1.0.0";

/// Maximum number of concurrently open streams of a session.
///
/// Each stream holds a connection to the target and yamux buffers, so this bounds the resources
/// used by one session. The value is a judgement call (generous for interactive use), not a
/// protocol limit. Streams over the limit are reset individually.
pub const MAX_STREAMS: usize = 64;
/// Maximum time to wait for the rest of a partially received header
const HEADER_TIMEOUT: Duration = Duration::from_secs(1);
/// Maximum time for the client to select a protocol
const NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    /// The client started a multistream-select negotiation, with the data read so far
    Multiplex(Vec<u8>),
    /// Normal passthrough, with the client data already read from the websocket
    Passthrough(Vec<u8>),
}

/// Decides the mode of a session from the first data of the client or the target.
///
/// `target_has_data` must complete when the target has sent data, without consuming it (e.g. by
/// peeking). A spurious completion selects passthrough, even if the client requests multiplexing.
pub async fn negotiate<R, F>(ws_reader: &mut R, target_has_data: F) -> io::Result<Mode>
where
    R: AsyncRead + Unpin,
    F: std::future::Future<Output = io::Result<usize>>,
{
    let mut received = Vec::new();
    let mut buf = [0u8; 64];
    tokio::pin!(target_has_data);
    loop {
        let partial_header_timeout = async {
            if received.is_empty() {
                std::future::pending::<()>().await
            } else {
                tokio::time::sleep(HEADER_TIMEOUT).await
            }
        };
        tokio::select! {
            n = ws_reader.read(&mut buf) => {
                let n = n?;
                received.extend_from_slice(&buf[..n]);
                if n == 0 {
                    return Ok(Mode::Passthrough(received));
                }
                if received.starts_with(MULTISTREAM_HEADER) {
                    return Ok(Mode::Multiplex(received));
                }
                if !MULTISTREAM_HEADER.starts_with(&received) {
                    return Ok(Mode::Passthrough(received));
                }
            }
            _ = &mut target_has_data => return Ok(Mode::Passthrough(received)),
            _ = partial_header_timeout => return Ok(Mode::Passthrough(received)),
        }
    }
}

/// Runs a multiplexing session until the websocket is closed.
///
/// `received` is the data already read from the websocket by [`negotiate`].
pub async fn run<T>(io: T, received: Vec<u8>, target: String) -> io::Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = Prefixed::new(received, io);
    let negotiation = multistream_select::listener_select_proto(io, [YAMUX_PROTOCOL]);
    let (_, io) = tokio::time::timeout(NEGOTIATION_TIMEOUT, negotiation)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "protocol negotiation timed out"))?
        .map_err(io::Error::other)?;

    let mut config = yamux::Config::default();
    // yamux terminates the whole session when its limit is exceeded, so it is set above
    // MAX_STREAMS, which is enforced below by resetting only the streams over the limit
    config.set_max_num_streams(2 * MAX_STREAMS);
    // yamux writes the header and the body of a frame separately and flushes once its queue is
    // drained; buffering coalesces them, so a frame does not become two websocket messages
    let io = futures_util::io::BufWriter::with_capacity(64 * 1024, io);
    let mut connection = yamux::Connection::new(io, config, yamux::Mode::Server);
    // dropping the set (when the session ends) aborts the forwarding tasks
    let mut tasks = tokio::task::JoinSet::new();

    loop {
        // polling for inbound streams also drives all I/O of the session
        match futures::future::poll_fn(|cx| connection.poll_next_inbound(cx)).await {
            Some(Ok(stream)) => {
                // reap finished forwarding tasks, so only open streams are counted
                while tasks.try_join_next().is_some() {}
                if tasks.len() >= MAX_STREAMS {
                    // dropping the stream resets it
                    drop(stream);
                    continue;
                }
                tasks.spawn(forward(stream, target.clone()));
            }
            Some(Err(err)) => return Err(io::Error::other(err)),
            None => return Ok(()),
        }
    }
}

/// Replays data that has already been read before reading from the inner stream
struct Prefixed<T> {
    prefix: Vec<u8>,
    position: usize,
    inner: T,
}

impl<T> Prefixed<T> {
    fn new(prefix: Vec<u8>, inner: T) -> Self {
        Prefixed {
            prefix,
            position: 0,
            inner,
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for Prefixed<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let remaining = &self.prefix[self.position..];
        if remaining.is_empty() {
            return Pin::new(&mut self.inner).poll_read(cx, buf);
        }
        let n = remaining.len().min(buf.len());
        buf[..n].copy_from_slice(&remaining[..n]);
        self.position += n;
        Poll::Ready(Ok(n))
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Prefixed<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

/// Connects a stream to the target and forwards data in both directions
async fn forward(stream: yamux::Stream, target: String) {
    let Ok(socket) = TcpStream::connect(&target).await else {
        // dropping the stream resets it
        return;
    };
    let (mut stream_reader, mut stream_writer) = stream.split();
    let (target_reader, target_writer) = socket.into_split();
    let (mut target_reader, mut target_writer) = (target_reader.compat(), target_writer.compat());

    let to_target = async {
        let _ = futures_util::io::copy(&mut stream_reader, &mut target_writer).await;
        let _ = target_writer.close().await;
    };
    let to_client = async {
        let _ = crate::proxy::copy_and_flush(&mut target_reader, &mut stream_writer).await;
        let _ = stream_writer.close().await;
    };
    futures::join!(to_target, to_client);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt as _;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn client_data_other_than_the_header_selects_passthrough() {
        let (mut client, plugin) = tokio::io::duplex(1024);
        client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();

        let mode = negotiate(&mut plugin.compat(), std::future::pending())
            .await
            .unwrap();

        assert_eq!(mode, Mode::Passthrough(b"GET / HTTP/1.1\r\n".to_vec()));
    }

    #[tokio::test]
    async fn target_sending_first_selects_passthrough() {
        let (_client, plugin) = tokio::io::duplex(1024);

        let mode = negotiate(&mut plugin.compat(), async { Ok(1) })
            .await
            .unwrap();

        assert_eq!(mode, Mode::Passthrough(Vec::new()));
    }

    #[tokio::test]
    async fn multistream_header_selects_multiplexing() {
        let (mut client, plugin) = tokio::io::duplex(1024);
        let mut plugin = plugin.compat();
        let negotiation = negotiate(&mut plugin, std::future::pending());
        let send = async {
            // the header may arrive in several parts, possibly together with the protocol proposal
            client.write_all(&MULTISTREAM_HEADER[..3]).await.unwrap();
            client.flush().await.unwrap();
            tokio::task::yield_now().await;
            client.write_all(&MULTISTREAM_HEADER[3..]).await.unwrap();
            client.write_all(b"\x0d/yamux/1.0.0\n").await.unwrap();
        };

        let (mode, _) = tokio::join!(negotiation, send);

        let Mode::Multiplex(received) = mode.unwrap() else {
            panic!("expected multiplexing");
        };
        assert!(received.starts_with(MULTISTREAM_HEADER));
    }

    #[tokio::test]
    async fn incomplete_header_falls_back_to_passthrough() {
        let (mut client, plugin) = tokio::io::duplex(1024);
        client.write_all(&MULTISTREAM_HEADER[..1]).await.unwrap();

        let mode = negotiate(&mut plugin.compat(), std::future::pending())
            .await
            .unwrap();

        assert_eq!(mode, Mode::Passthrough(MULTISTREAM_HEADER[..1].to_vec()));
    }

    #[tokio::test]
    async fn other_protocols_are_declined() {
        let (client, plugin) = tokio::io::duplex(1024);
        tokio::spawn(run(plugin.compat(), Vec::new(), "127.0.0.1:1".into()));

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            multistream_select::dialer_select_proto(
                client.compat(),
                ["/mplex/6.7.0"],
                multistream_select::Version::V1,
            ),
        )
        .await
        .expect("the negotiation should complete");

        assert!(matches!(
            result,
            Err(multistream_select::NegotiationError::Failed)
        ));
    }

    #[tokio::test]
    async fn streams_are_connected_to_the_target() {
        let target = echo_server().await;
        let mut session = start_session(target).await;
        let mut first = session.open().await;
        let mut second = session.open().await;
        session.drive();

        tokio::time::timeout(Duration::from_secs(5), async {
            first.write_all(b"hello").await.unwrap();
            second.write_all(b"world").await.unwrap();
            let mut buf = [0; 5];
            first.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");
            second.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"world");

            // half-close: the echo server sees the end of its input and closes its side
            first.close().await.unwrap();
            let mut rest = Vec::new();
            first.read_to_end(&mut rest).await.unwrap();
            assert!(rest.is_empty());
        })
        .await
        .expect("streams should be forwarded to the target");
    }

    #[tokio::test]
    async fn target_connection_failures_reset_the_stream() {
        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = unused.local_addr().unwrap().to_string();
        drop(unused);
        let mut session = start_session(target).await;
        let mut stream = session.open().await;
        session.drive();

        tokio::time::timeout(Duration::from_secs(5), async {
            stream.write_all(b"hello").await.unwrap();
            let mut buf = Vec::new();
            stream.read_to_end(&mut buf).await.unwrap();
            assert!(buf.is_empty());

            // if the stream is truly reset, we won't be able to write either
            let write = stream.write_all(b"more").await;
            assert!(
                write.is_err(),
                "the stream should be reset, not half-closed"
            );
        })
        .await
        .expect("the stream should be closed");
    }

    #[tokio::test]
    async fn streams_over_the_limit_are_rejected_without_closing_open_streams() {
        let target = echo_server().await;
        let mut session = start_session(target).await;
        let mut streams = Vec::new();
        for _ in 0..MAX_STREAMS {
            streams.push(session.open().await);
        }
        let mut over_limit = session.open().await;
        session.drive();

        // writing makes the plugin aware of a stream, so the streams are accepted in this order
        for (i, stream) in streams.iter_mut().enumerate() {
            assert!(
                echoes(stream).await,
                "stream {i} should echo before the limit is hit"
            );
        }
        assert!(
            !echoes(&mut over_limit).await,
            "the stream over the limit should be rejected"
        );
        for (i, stream) in streams.iter_mut().enumerate() {
            assert!(
                echoes(stream).await,
                "stream {i} should still echo after a stream was rejected"
            );
        }
    }

    #[tokio::test]
    async fn streams_can_be_opened_again_once_others_are_closed() {
        let target = echo_server().await;
        let mut session = start_session(target).await;
        let mut streams = Vec::new();
        for _ in 0..MAX_STREAMS {
            streams.push(session.open().await);
        }
        let mut closed = streams.pop().unwrap();
        let mut replacement = session.open().await;
        session.drive();

        for stream in streams.iter_mut() {
            assert!(echoes(stream).await);
        }
        assert!(echoes(&mut closed).await);
        tokio::time::timeout(Duration::from_secs(5), async {
            closed.close().await.unwrap();
            closed.read_to_end(&mut Vec::new()).await.unwrap();
        })
        .await
        .expect("the stream should be closed");

        assert!(
            echoes(&mut replacement).await,
            "a stream should be accepted once another one is closed"
        );
    }

    /// Returns true if the stream echoes data, false if the stream is closed or reset
    async fn echoes(stream: &mut yamux::Stream) -> bool {
        tokio::time::timeout(Duration::from_secs(5), async {
            stream.write_all(b"x").await?;
            let mut buf = [0; 1];
            stream.read_exact(&mut buf).await
        })
        .await
        .expect("the stream should either echo or be rejected")
        .is_ok()
    }

    struct Session {
        connection: Option<
            yamux::Connection<
                multistream_select::Negotiated<async_compat::Compat<tokio::io::DuplexStream>>,
            >,
        >,
    }

    impl Session {
        async fn open(&mut self) -> yamux::Stream {
            let connection = self.connection.as_mut().unwrap();
            futures::future::poll_fn(|cx| connection.poll_new_outbound(cx))
                .await
                .unwrap()
        }

        /// drives the client side of the session in the background
        fn drive(&mut self) {
            let mut connection = self.connection.take().unwrap();
            tokio::spawn(async move {
                while futures::future::poll_fn(|cx| connection.poll_next_inbound(cx))
                    .await
                    .is_some()
                {}
            });
        }
    }

    async fn start_session(target: String) -> Session {
        let (client, plugin) = tokio::io::duplex(64 * 1024);
        let mut plugin = plugin.compat();
        let client = client.compat();
        // as in the plugin: the mode is detected first, then the negotiation continues
        tokio::spawn(async move {
            let negotiation = negotiate(&mut plugin, std::future::pending());
            let Ok(Mode::Multiplex(received)) = negotiation.await else {
                panic!("expected multiplexing");
            };
            let _ = run(plugin, received, target).await;
        });
        let (protocol, client) = multistream_select::dialer_select_proto(
            client,
            [YAMUX_PROTOCOL],
            multistream_select::Version::V1,
        )
        .await
        .unwrap();
        assert_eq!(protocol, YAMUX_PROTOCOL);
        Session {
            connection: Some(yamux::Connection::new(
                client,
                yamux::Config::default(),
                yamux::Mode::Client,
            )),
        }
    }

    async fn echo_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let (mut reader, mut writer) = socket.split();
                    let _ = tokio::io::copy(&mut reader, &mut writer).await;
                    let _ = writer.shutdown().await;
                });
            }
        });
        address
    }
}
