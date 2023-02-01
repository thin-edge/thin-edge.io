use async_compat::CompatExt;
use async_tungstenite::tokio::ConnectStream;
use futures::future::join;
use futures::future::select;
use futures_util::io::AsyncReadExt;
use futures_util::io::AsyncWriteExt;
use miette::Context;
use miette::Diagnostic;
use miette::IntoDiagnostic;
use rand::RngCore;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;

use url::Url;
use ws_stream_tungstenite::WsStream;

use crate::auth::Jwt;
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
        jwt: Jwt,
    ) -> miette::Result<Self> {
        let socket_future = TcpStream::connect(socket);
        let websocket_future = Websocket::new(url, jwt.authorization_header());

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
    socket: WsStream<ConnectStream>,
}

fn generate_sec_websocket_key() -> String {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    base64::encode(bytes)
}

impl Websocket {
    async fn new(url: &Url, authorization: String) -> miette::Result<Self> {
        let request = http::Request::builder()
            .header("Authorization", authorization)
            .header("Sec-WebSocket-Key", generate_sec_websocket_key())
            .header("Host", url.host_str().unwrap())
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .uri(url.to_string())
            .body(())
            .into_diagnostic()
            .context("Instantiating Websocket connection")?;

        let socket = async_tungstenite::tokio::connect_async(request)
            .await
            .into_diagnostic()
            .context("Connecting to Websocket")?
            .0;
        Ok(Websocket {
            socket: WsStream::new(socket),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_is_base64_encoded_16_byte_sequence() {
        let key = generate_sec_websocket_key();

        let decoded = base64::decode(key).unwrap();

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
}
