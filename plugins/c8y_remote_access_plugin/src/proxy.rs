use crate::auth::Auth;
use async_compat::CompatExt;
use async_http_proxy::http_connect_tokio;
use async_http_proxy::http_connect_tokio_with_basic_auth;
use async_tungstenite::tokio::ClientStream;
use base64::prelude::*;
use futures::future::join;
use futures::future::select;
use futures_util::io::AsyncReadExt;
use futures_util::io::AsyncWriteExt;
use http::HeaderValue;
use miette::miette;
use miette::Context;
use miette::Diagnostic;
use miette::IntoDiagnostic;
use rand::RngCore;
use rustls::ClientConfig;
use std::pin::Pin;
use std::sync::Arc;
use tedge_config::all_or_nothing;
use tedge_config::models::proxy_scheme::ProxyScheme;
use tedge_config::tedge_toml::TEdgeConfigReaderProxy;
use thiserror::Error;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use url::Url;
use ws_stream_tungstenite::WsStream;

use crate::SUCCESS_MESSAGE;

/// This proxy creates a TCP connection to a local socket and creates a websocket. Cumulocity cloud will initiate a
/// connection to the websocket. Any data received from the socket is sent out via the websocket and any data received
/// from the websocket is sent to the local socket.
pub struct WebsocketSocketProxy {
    socket: TcpStream,
    websocket: Websocket,
}

#[derive(Diagnostic, Error, Debug)]
#[error("Failed to connect to TCP socket")]
struct SocketError(#[from] std::io::Error);

impl WebsocketSocketProxy {
    pub async fn connect<SA: ToSocketAddrs + std::fmt::Debug>(
        url: &Url,
        socket: SA,
        auth: Auth,
        config: Option<ClientConfig>,
        proxy: &TEdgeConfigReaderProxy,
    ) -> miette::Result<Self> {
        let socket_future = TcpStream::connect(socket);
        let websocket_future = Websocket::new(url, auth.authorization_header(), config, proxy);

        match join(socket_future, websocket_future).await {
            (Err(socket_error), _) => Err(SocketError(socket_error))?,
            (_, Err(websocket_error)) => Err(websocket_error),
            (Ok(socket), Ok(websocket)) => {
                println!("{SUCCESS_MESSAGE}");
                Ok(WebsocketSocketProxy { socket, websocket })
            }
        }
    }

    pub async fn run(mut self) {
        let (mut ws_reader, mut ws_writer) = self.websocket.socket.split();
        let (mut reader, mut writer) = self.socket.split();
        let (mut reader, mut writer) = (reader.compat_mut(), writer.compat_mut());
        let incoming = futures_util::io::copy(&mut ws_reader, &mut writer);
        let outgoing = futures_util::io::copy(&mut reader, &mut ws_writer);
        {
            futures::pin_mut!(incoming);
            futures::pin_mut!(outgoing);

            select(incoming, outgoing).await;
        }
        println!("STOPPING");
        let _ = join(ws_writer.close(), writer.close()).await;
    }
}

struct Websocket {
    socket: WsStream<ClientStream<MaybeTlsStream>>,
}

fn generate_sec_websocket_key() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    BASE64_STANDARD.encode(bytes)
}

enum MaybeTlsStream {
    Plain(TcpStream),
    Rustls(Box<TlsStream<TcpStream>>),
}

impl AsyncRead for MaybeTlsStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(ref mut tcp) => Pin::new(tcp).poll_read(cx, buf),
            Self::Rustls(ref mut tcp) => Pin::new(tcp).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            Self::Plain(ref mut tcp) => Pin::new(tcp).poll_write(cx, buf),
            Self::Rustls(ref mut tcp) => Pin::new(tcp).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Plain(ref mut tcp) => Pin::new(tcp).poll_flush(cx),
            Self::Rustls(ref mut tcp) => Pin::new(tcp).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Plain(ref mut tcp) => Pin::new(tcp).poll_shutdown(cx),
            Self::Rustls(ref mut tcp) => Pin::new(tcp).poll_shutdown(cx),
        }
    }
}

impl Websocket {
    async fn new(
        url: &Url,
        authorization: HeaderValue,
        config: Option<ClientConfig>,
        proxy: &TEdgeConfigReaderProxy,
    ) -> miette::Result<Self> {
        let config = config.map(Arc::new);
        let target_host = url
            .host_str()
            .ok_or(miette!("{url} does not contain a host"))?;
        let target_port = url
            .port_or_known_default()
            .ok_or(miette!("{url} does not contain a port"))?;
        let stream = if let Some(address) = proxy.address.or_none() {
            let host_port = format!("{}:{}", address.host(), address.port());
            let stream = TcpStream::connect(&host_port).await.into_diagnostic()?;
            let mut stream = match address.scheme() {
                ProxyScheme::Https => {
                    let connector: TlsConnector = config.clone().unwrap().into();
                    MaybeTlsStream::Rustls(Box::new(
                        connector
                            .connect(host_port.try_into().unwrap(), stream)
                            .await
                            .into_diagnostic()?,
                    ))
                }
                ProxyScheme::Http => MaybeTlsStream::Plain(stream),
            };
            if let Some((username, password)) =
                all_or_nothing((proxy.username.as_ref(), proxy.password.as_ref()))
                    .map_err(|e| miette!(e))?
            {
                http_connect_tokio_with_basic_auth(
                    &mut stream,
                    target_host,
                    target_port,
                    username,
                    password,
                )
                .await
                .into_diagnostic()?;
            } else {
                http_connect_tokio(&mut stream, target_host, target_port)
                    .await
                    .into_diagnostic()?;
            }
            stream
        } else {
            MaybeTlsStream::Plain(
                TcpStream::connect(format!("{target_host}:{target_port}",))
                    .await
                    .into_diagnostic()?,
            )
        };
        let request = http::Request::builder()
            .header("Authorization", authorization)
            .header("Sec-WebSocket-Key", generate_sec_websocket_key())
            .header("Host", url.host_str().unwrap())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-protocol", "binary")
            .uri(url.to_string())
            .body(())
            .into_diagnostic()
            .context("Instantiating Websocket connection")?;

        let connector = config.map(|c| c.into());
        let socket = async_tungstenite::tokio::client_async_tls_with_connector_and_config(
            request, stream, connector, None,
        )
        .await
        .into_diagnostic()
        .with_context(|| format!("host {url}"))
        .context("Connecting to Websocket")?
        .0;

        Ok(Websocket {
            socket: WsStream::new(socket),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::extract::ws::WebSocket;
    use axum::extract::WebSocketUpgrade;
    use axum::response::Response;
    use axum::routing::any;
    use axum::Router;
    use http::HeaderMap;
    use http::HeaderName;
    use http::StatusCode;
    use sha1::Digest;
    use tedge_config::TEdgeConfig;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;

    use super::*;

    #[test]
    fn generated_key_is_base64_encoded_16_byte_sequence() {
        let key = generate_sec_websocket_key();

        let decoded = BASE64_STANDARD.decode(key).unwrap();

        assert_eq!(decoded.len(), 16);
    }

    #[test]
    fn generated_key_is_ascii() {
        let key = generate_sec_websocket_key();

        assert!(key.is_ascii());
    }

    #[test]
    fn generated_keys_are_unique_per_connection() {
        let key_1 = generate_sec_websocket_key();
        let key_2 = generate_sec_websocket_key();

        assert_ne!(key_1, key_2);
    }

    #[tokio::test]
    async fn websocket_connection_copes_with_forced_subprotocol() {
        let app = Router::new().route("/ws", any(handler));

        // We want to test what happens when replicating Cumulocity's headers
        // so avoid the axum websocket handling and send the upgrade manually
        // After that's been sent, we can disconnect immediately and everything
        // is valid
        async fn handler(mut headers: HeaderMap) -> (HeaderMap, StatusCode) {
            let key = headers.remove("sec-websocket-key").unwrap();
            (
                [
                    (
                        HeaderName::from_static("connection"),
                        HeaderValue::from_static("upgrade"),
                    ),
                    (
                        HeaderName::from_static("upgrade"),
                        HeaderValue::from_static("websocket"),
                    ),
                    (
                        HeaderName::from_static("sec-websocket-accept"),
                        sign(key.as_bytes()),
                    ),
                    // Cumulocity always forces `sec-websocket-protocol=binary`
                    (
                        HeaderName::from_static("sec-websocket-protocol"),
                        HeaderValue::from_static("binary"),
                    ),
                    (
                        HeaderName::from_static("x-content-type-options"),
                        HeaderValue::from_static("nosniff"),
                    ),
                    (
                        HeaderName::from_static("x-xss-protection"),
                        HeaderValue::from_static("1; mode=block"),
                    ),
                    (
                        HeaderName::from_static("cache-control"),
                        HeaderValue::from_static("no-cache, no-store, max-age=0, must-revalidate"),
                    ),
                    (
                        HeaderName::from_static("pragma"),
                        HeaderValue::from_static("no-cache"),
                    ),
                    (
                        HeaderName::from_static("expires"),
                        HeaderValue::from_static("0"),
                    ),
                ]
                .into_iter()
                .collect(),
                StatusCode::SWITCHING_PROTOCOLS,
            )
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let axum_port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_port = target.local_addr().unwrap().port();
        let tedge_config = TEdgeConfig::load_toml_str("");
        let proxy = WebsocketSocketProxy::connect(
            &format!("ws://127.0.0.1:{axum_port}/ws").parse().unwrap(),
            format!("127.0.0.1:{target_port}"),
            Auth::test_value(HeaderValue::from_static("AUTHORIZATION HEADER")),
            None,
            &tedge_config.proxy,
        )
        .await
        .unwrap();

        proxy.run().await;
    }

    #[tokio::test]
    async fn websocket_connection() {
        let app = Router::new().route("/ws", any(handler));

        async fn handler(ws: WebSocketUpgrade) -> Response {
            ws.protocols(["binary"]).on_upgrade(handle_socket)
        }

        async fn handle_socket(mut socket: WebSocket) {
            use axum::extract::ws::Message;
            match socket.recv().await {
                Some(Ok(Message::Binary(msg))) => {
                    assert_eq!(std::str::from_utf8(&msg).unwrap(), "tcp->ws")
                }
                Some(Ok(msg)) => panic!("Expected `Message::Binary(_)`, got {msg:?}"),
                Some(Err(err)) => panic!("{err:?}"),
                None => panic!("WebSocket closed unexpectedly"),
            }

            socket
                .send(Message::Binary("ws->tcp".into()))
                .await
                .unwrap();
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let axum_port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_port = target.local_addr().unwrap().port();
        let assert_bidirectional_comms = tokio::spawn(async move {
            let (mut data, _) = target.accept().await.unwrap();
            data.write_all("tcp->ws".as_bytes()).await.unwrap();
            data.flush().await.unwrap();
            let mut incoming = String::new();
            data.read_to_string(&mut incoming).await.unwrap();
            assert_eq!(incoming, "ws->tcp");
        });
        let tedge_config = TEdgeConfig::load_toml_str("");

        tokio::time::timeout(Duration::from_secs(5), async move {
            let proxy = WebsocketSocketProxy::connect(
                &format!("ws://127.0.0.1:{axum_port}/ws").parse().unwrap(),
                format!("127.0.0.1:{target_port}"),
                Auth::test_value(HeaderValue::from_static("AUTHORIZATION HEADER")),
                None,
                &tedge_config.proxy,
            )
            .await
            .unwrap();
            proxy.run().await;
            assert_bidirectional_comms.await.unwrap();
        })
        .await
        .unwrap();
    }

    fn sign(key: &[u8]) -> HeaderValue {
        let mut sha1 = sha1::Sha1::default();
        sha1.update(key);
        sha1.update(&b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"[..]);
        let b64 = bytes::Bytes::from(BASE64_STANDARD.encode(sha1.finalize()));
        HeaderValue::from_maybe_shared(b64).expect("base64 is a valid value")
    }
}
