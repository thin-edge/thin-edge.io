use async_compat::CompatExt;
use futures::future::join;
use futures::future::select;
use futures_util::io::AsyncReadExt;
use futures_util::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::net::ToSocketAddrs;

use url::Url;

use crate::auth::Jwt;
use crate::Websocket;

/// This proxy creates a TCP connection to a local socket and creates a websocket. Cumulocity cloud will initiate a
/// connection to the websocket. Any data received from the socket is sent out via the websocket and any data received
/// from the websocket is sent to the local socket.
pub struct WebsocketSocketProxy {
    socket: TcpStream,
    websocket: Websocket,
}

impl WebsocketSocketProxy {
    pub async fn connect<SA: ToSocketAddrs + std::fmt::Debug>(
        url: &Url,
        socket: SA,
        jwt: Jwt,
    ) -> Result<Self, std::io::Error> {
        let socket_future = TcpStream::connect(socket);
        let websocket_future = Websocket::new(url, jwt.authorization_header());

        match join(socket_future, websocket_future).await {
            (Err(socket_error), _) => panic!("{socket_error}"),
            (_, Err(websocket_error)) => panic!("{websocket_error}"),
            (Ok(socket), Ok(websocket)) => Ok(WebsocketSocketProxy { socket, websocket }),
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
