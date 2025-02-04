use crate::tokens::*;
use anyhow::Context;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::WebSocket;
use axum::extract::FromRef;
use axum::extract::Path;
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use axum_tls::config::load_ssl_config;
use axum_tls::config::PemReader;
use axum_tls::config::TrustStoreLoader;
use axum_tls::start_tls_server;
use futures::future::BoxFuture;
use futures::FutureExt;
use futures::Sink;
use futures::SinkExt;
use futures::Stream;
use futures::StreamExt;
use http_body::Frame;
use http_body_util::Full;
use http_body_util::StreamBody;
use hyper::header::AUTHORIZATION;
use hyper::header::HOST;
use hyper::HeaderMap;
use reqwest::Method;
use reqwest::StatusCode;
use std::error::Error;
use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::net::TcpListener;
use std::sync::Arc;
use tedge_config_macros::OptionalConfig;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tracing::error;
use tracing::info;

pub struct Server {
    fut: BoxFuture<'static, std::io::Result<()>>,
}

impl Server {
    pub(crate) fn try_init(
        state: AppData,
        address: IpAddr,
        port: u16,
        cert_path: OptionalConfig<impl PemReader>,
        key_path: OptionalConfig<impl PemReader>,
        ca_path: OptionalConfig<impl TrustStoreLoader>,
    ) -> anyhow::Result<Self> {
        let app = create_app(state);
        let server_config = load_ssl_config(cert_path, key_path, ca_path, "Cumulocity proxy")?;
        let fut = if let Some(server_config) = server_config {
            try_bind_with_tls(app, address, port, server_config)?.boxed()
        } else {
            try_bind_insecure(app, address, port)?.boxed()
        };

        Ok(Server { fut })
    }

    pub fn wait(self) -> BoxFuture<'static, std::io::Result<()>> {
        self.fut
    }
}

struct ProxyError(anyhow::Error);

impl From<anyhow::Error> for ProxyError {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        error!("{:?}", self.0);
        (
            StatusCode::BAD_GATEWAY,
            "Error communicating with Cumulocity",
        )
            .into_response()
    }
}

fn create_app(state: AppData) -> Router<()> {
    let handle = get(respond_to)
        .post(respond_to)
        .put(respond_to)
        .patch(respond_to)
        .delete(respond_to)
        .options(respond_to);
    Router::new()
        .route("/c8y", handle.clone())
        .route("/c8y/", handle.clone())
        .route("/c8y/{*path}", handle)
        .with_state(AppState::from(state))
}

fn try_bind_insecure(
    app: Router<()>,
    address: IpAddr,
    port: u16,
) -> anyhow::Result<impl Future<Output = io::Result<()>>> {
    info!("Launching on port {port} with HTTP");
    let listener =
        TcpListener::bind((address, port)).with_context(|| format!("binding to port {port}"))?;
    Ok(axum_server::from_tcp(listener).serve(app.into_make_service()))
}

fn try_bind_with_tls(
    app: Router<()>,
    address: IpAddr,
    port: u16,
    server_config: rustls::ServerConfig,
) -> anyhow::Result<impl Future<Output = io::Result<()>>> {
    info!("Launching on port {port} with HTTPS");
    let listener =
        TcpListener::bind((address, port)).with_context(|| format!("binding to port {port}"))?;
    Ok(start_tls_server(listener, server_config, app))
}

pub(crate) struct AppData {
    pub is_https: bool,
    pub host: String,
    pub token_manager: SharedTokenManager,
    pub client: reqwest::Client,
}

#[derive(Clone)]
struct AppState {
    target_host: TargetHost,
    client: reqwest::Client,
    token_manager: SharedTokenManager,
}

impl From<AppData> for AppState {
    fn from(value: AppData) -> Self {
        let (http, ws) = if value.is_https {
            ("https", "wss")
        } else {
            ("http", "ws")
        };
        let host = value.host;
        AppState {
            target_host: TargetHost {
                http: format!("{http}://{host}").into(),
                ws: format!("{ws}://{host}").into(),
                without_scheme: host.into(),
            },
            token_manager: value.token_manager,
            client: value.client,
        }
    }
}

impl FromRef<AppState> for TargetHost {
    fn from_ref(input: &AppState) -> Self {
        input.target_host.clone()
    }
}

impl FromRef<AppState> for SharedTokenManager {
    fn from_ref(input: &AppState) -> Self {
        input.token_manager.clone()
    }
}

impl FromRef<AppState> for reqwest::Client {
    fn from_ref(input: &AppState) -> Self {
        input.client.clone()
    }
}

#[derive(Clone)]
struct TargetHost {
    http: Arc<str>,
    ws: Arc<str>,
    without_scheme: Arc<str>,
}

fn axum_to_tungstenite(message: axum::extract::ws::Message) -> tungstenite::Message {
    use axum::extract::ws::CloseFrame as InCf;
    use axum::extract::ws::Message as In;
    use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame as OutCf;
    use tokio_tungstenite::tungstenite::Message as Out;
    match message {
        In::Text(t) => Out::Text(t.as_str().into()),
        In::Binary(t) => Out::Binary(t),
        In::Ping(t) => Out::Ping(t),
        In::Pong(t) => Out::Pong(t),
        In::Close(Some(InCf { code, reason })) => Out::Close(Some(OutCf {
            code: code.into(),
            reason: reason.as_str().into(),
        })),
        In::Close(None) => Out::Close(None),
    }
}

fn tungstenite_to_axum(message: tungstenite::Message) -> axum::extract::ws::Message {
    use axum::extract::ws::CloseFrame as OutCf;
    use axum::extract::ws::Message as Out;
    use tokio_tungstenite::tungstenite::protocol::frame::CloseFrame as InCf;
    use tokio_tungstenite::tungstenite::Message as In;
    match message {
        In::Text(t) => Out::Text(t.as_str().into()),
        In::Binary(t) => Out::Binary(t),
        In::Ping(t) => Out::Ping(t),
        In::Pong(t) => Out::Pong(t),
        In::Close(Some(InCf { code, reason })) => Out::Close(Some(OutCf {
            code: code.into(),
            reason: reason.as_str().into(),
        })),
        In::Close(None) => Out::Close(None),
        In::Frame(_) => unreachable!("This function is only called when reading a message"),
    }
}

async fn connect_to_websocket(
    auth_value: &str,
    headers: &HeaderMap<HeaderValue>,
    uri: &str,
    host: &TargetHost,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Error> {
    let mut req = Request::builder();
    for (name, value) in headers {
        req = req.header(name.as_str(), value);
    }
    req = req.header(AUTHORIZATION, auth_value);
    let req = req
        .uri(uri)
        .header(HOST, host.without_scheme.as_ref())
        .body(())
        .expect("Builder should always work as the headers are copied from a previous request, so must be valid");
    tokio_tungstenite::connect_async(req)
        .await
        .map(|(c8y, _)| c8y)
}

async fn proxy_ws(
    mut ws: WebSocket,
    host: TargetHost,
    retrieve_token: State<SharedTokenManager>,
    headers: HeaderMap<HeaderValue>,
    path: String,
) {
    use axum::extract::ws::CloseFrame;
    use tungstenite::error::Error;
    let uri = format!("{}/{path}", host.ws);

    let c8y = {
        match retrieve_token.not_matching(None).await {
            Ok(token) => match connect_to_websocket(&token, &headers, &uri, &host).await {
                Ok(c8y) => Ok(c8y),
                Err(Error::Http(res)) if res.status() == StatusCode::UNAUTHORIZED => {
                    match retrieve_token.not_matching(Some(&token)).await {
                        Ok(token) => {
                            match connect_to_websocket(&token, &headers, &uri, &host).await {
                                Ok(c8y) => Ok(c8y),
                                Err(e) => Err(anyhow::Error::from(e)
                                    .context("Failed to connect to proxied websocket")),
                            }
                        }
                        Err(e) => Err(e.context("Failed to retrieve JWT token")),
                    }
                }
                Err(e) => Err(anyhow::Error::from(e)),
            },
            Err(e) => Err(e.context("Failed to retrieve JWT token")),
        }
    }
    .context("Error connecting to proxied websocket");

    let c8y = match c8y {
        Err(e) => {
            let _ = ws
                .send(axum::extract::ws::Message::Close(Some(CloseFrame {
                    code: 1014,
                    reason: "Failed to connect to Cumulocity".into(),
                })))
                .await;
            error!("{e:?}");
            return;
        }
        Ok(c8y) => c8y,
    };
    let (mut to_c8y, mut from_c8y) = c8y.split();
    let (mut to_client, mut from_client) = ws.split();

    let (tx_c_to_c8y, mut rx_c_to_c8y) = mpsc::channel::<()>(1);
    let mut client_to_c8y = tokio::spawn(async move {
        use tungstenite::protocol::frame::CloseFrame;
        use tungstenite::Message;
        let extract_close_frame = |msg| match msg {
            Message::Close(cf) => Ok(cf),
            msg => Err(msg),
        };

        let mut res = tokio::select! {
            res = copy_messages_from(&mut from_client, &mut to_c8y, axum_to_tungstenite, extract_close_frame) => res,
            _ = rx_c_to_c8y.recv() => Ok(None),
        };

        let close_frame = match &mut res {
            Ok(close_frame) => close_frame.take(),
            Err(_) => Some(CloseFrame {
                code: CloseCode::Bad(1014),
                reason: "Error communicating with Cumulocity".into(),
            }),
        };
        let _ = to_c8y.send(Message::Close(close_frame)).await;
        info!("Closed websocket proxy from client to Cumulocity");
        res
    });

    let (tx_c8y_to_c, mut rx_c8y_to_c) = mpsc::channel::<()>(1);
    let mut c8y_to_client = tokio::spawn(async move {
        use axum::extract::ws::Message;
        let extract_close_frame = |msg| match msg {
            Message::Close(cf) => Ok(cf),
            msg => Err(msg),
        };

        let mut res = tokio::select! {
            res = copy_messages_from(&mut from_c8y, &mut to_client, tungstenite_to_axum, extract_close_frame) => res,
            _ = rx_c8y_to_c.recv() => Ok(None),
        };

        let close_frame = match &mut res {
            Ok(close_frame) => close_frame.take(),
            Err(_) => Some(CloseFrame {
                code: 1014,
                reason: "Error communicating with Cumulocity".into(),
            }),
        };
        let _ = to_client.send(Message::Close(close_frame)).await;
        info!("Closed websocket proxy from Cumulocity to client");
        res
    });

    tokio::select! {
        res = (&mut client_to_c8y) => {
            if let Err(e) = res.unwrap() {
                error!("Websocket error proxying messages from the client to Cumulocity: {e:?}");
            }
            let _ = tx_c8y_to_c.send(()).await;
        }
        res = (&mut c8y_to_client) => {
            if let Err(e) = res.unwrap() {
                error!("Websocket error proxying messages from Cumulocity to the client: {e:?}");
            }
            let _ = tx_c_to_c8y.send(()).await;
        }
    }
}

async fn copy_messages_from<T, TErr, CloseFrame, U, UErr>(
    input: &mut (impl Stream<Item = Result<T, TErr>> + Unpin),
    output: &mut (impl Sink<U, Error = UErr> + Unpin),
    convert_message: fn(T) -> U,
    extract_close_frame: fn(U) -> Result<Option<CloseFrame>, U>,
) -> anyhow::Result<Option<CloseFrame>>
where
    TErr: Error + Sync + Send + 'static,
    UErr: Error + Sync + Send + 'static,
    U: std::fmt::Debug,
{
    while let Some(msg) = input.next().await {
        match msg.map(convert_message).map(extract_close_frame) {
            Ok(Ok(close_frame)) => return Ok(close_frame),
            Ok(Err(msg)) => output
                .send(msg)
                .await
                .context("Error sending websocket message")?,
            Err(err) => Err(err).context("Error receiving websocket message")?,
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
#[axum::debug_handler(state = AppState)]
async fn respond_to(
    State(host): State<TargetHost>,
    State(client): State<reqwest::Client>,
    retrieve_token: State<SharedTokenManager>,
    path: Option<Path<String>>,
    uri: hyper::Uri,
    method: Method,
    mut headers: HeaderMap<HeaderValue>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    small_body: crate::body::PossiblySmallBody,
) -> Result<Response, ProxyError> {
    let path = match &path {
        Some(Path(p)) => p.as_str(),
        None => "",
    };
    let auth: fn(reqwest::RequestBuilder, &str) -> reqwest::RequestBuilder =
        if headers.contains_key(AUTHORIZATION) {
            |req, _auth_value| req
        } else {
            |req, auth_value| req.header(AUTHORIZATION, auth_value)
        };
    headers.remove(HOST);

    // Cumulocity revokes the device token if we access parts of the frontend UI,
    // so deny requests to these proactively
    if path.ends_with(".js") || path.starts_with("apps/") {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    let mut destination = format!("{}/{path}", host.http);
    if let Some(query) = uri.query() {
        destination += "?";
        destination += query;
    }

    let mut token = retrieve_token
        .not_matching(None)
        .await
        .with_context(|| "failed to retrieve JWT token")?;

    if let Ok(ws) = ws {
        let path = path.to_owned();
        return Ok(ws.on_upgrade(|socket| proxy_ws(socket, host, retrieve_token, headers, path)));
    }
    let (body, body_clone) = small_body.try_clone();
    if body_clone.is_none() {
        let destination = format!("{}/tenant/currentTenant", host.http);
        let response = client
            .head(&destination)
            .header(AUTHORIZATION, token.to_string())
            .send()
            .await
            .with_context(|| format!("making HEAD request to {destination}"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            token = retrieve_token
                .not_matching(Some(&token))
                .await
                .with_context(|| "failed to retrieve JWT token")?;
        }
    }

    let send_request = |body, token: &str| {
        auth(
            client
                .request(method.to_owned(), &destination)
                .headers(headers.clone()),
            token,
        )
        .body(body)
        .send()
    };
    let mut res = send_request(reqwest::Body::wrap(body), &token)
        .await
        .with_context(|| format!("making proxied request to {destination}"))?;

    if res.status() == StatusCode::UNAUTHORIZED {
        token = retrieve_token
            .not_matching(Some(&token))
            .await
            .with_context(|| "failed to retrieve JWT token")?;
        if let Some(body) = body_clone {
            res = send_request(body.into(), &token)
                .await
                .with_context(|| format!("making proxied request to {destination}"))?;
        }
    }
    let te_header = res.headers_mut().remove("transfer-encoding");
    let status = res.status();
    let headers = std::mem::take(res.headers_mut());

    let body = if te_header.is_some_and(|h| h.to_str().unwrap_or_default().contains("chunked")) {
        axum::body::Body::new(StreamBody::new(
            res.bytes_stream().map(|b| b.map(Frame::data)),
        ))
    } else {
        axum::body::Body::new(Full::new(
            res.bytes().await.context("reading proxy response bytes")?,
        ))
    };

    Ok((status, headers, body).into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Bytes;
    use axum::http::Request;
    use axum::middleware::Next;
    use axum_extra::headers::authorization::Bearer;
    use axum_extra::headers::Authorization;
    use axum_extra::TypedHeader;
    use camino::Utf8PathBuf;
    use futures::channel::mpsc;
    use futures::future::ready;
    use futures::stream::once;
    use futures::Stream;
    use rustls::client::danger::HandshakeSignatureValid;
    use rustls::client::danger::ServerCertVerified;
    use rustls::pki_types::pem::PemObject as _;
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::PrivateKeyDer;
    use rustls::pki_types::ServerName;
    use rustls::pki_types::UnixTime;
    use std::borrow::Cow;
    use std::future::IntoFuture;
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;
    use tokio::sync::Mutex;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    use tokio_tungstenite::tungstenite::Message;
    use tokio_tungstenite::tungstenite::Message::Close;
    use tokio_tungstenite::Connector;
    use tokio_tungstenite::MaybeTlsStream;
    use tokio_tungstenite::WebSocketStream;

    use super::*;

    struct ConnectionClosed;

    fn websocket_app<Fut>(
        callback: fn(WebSocket) -> Fut,
    ) -> (
        Router,
        impl Future<Output = Result<Option<ConnectionClosed>, anyhow::Error>>,
    )
    where
        Fut: Future + Send + 'static,
    {
        let (mut tx, mut rx) = mpsc::channel(1);
        let test_app = Router::new().route(
            "/my/ws/endpoint",
            get(move |ws: WebSocketUpgrade| async move {
                ws.on_upgrade(move |ws| async move {
                    callback(ws).await;
                    tx.send(ConnectionClosed).await.unwrap();
                })
            }),
        );
        (
            test_app,
            tokio::time::timeout(Duration::from_secs(5), async move { rx.next().await })
                .map(|e| e.context("Waiting for ConnectionClosed from server")),
        )
    }

    async fn receive_all_messages(mut ws: impl Stream + Unpin) {
        while ws.next().await.is_some() {}
    }

    async fn drop_connection(_ws: WebSocket) {}

    async fn close_connection(mut ws: WebSocket) {
        ws.close().await.unwrap()
    }

    #[tokio::test]
    async fn does_not_forward_host_header_for_http_requests() {
        let target_host = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = target_host.local_addr().unwrap();

        let proxy_port = start_server_port(target.port(), vec!["unused token"]);
        tokio::spawn(async move {
            reqwest_client()
                .get(format!("http://127.0.0.1:{proxy_port}/c8y/test"))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
        });

        let proxy_host = format!("127.0.0.1:{proxy_port}");
        let destination_host = format!("127.0.0.1:{}", target.port());

        let (mut tcp_stream, _) =
            tokio::time::timeout(Duration::from_secs(5), target_host.accept())
                .await
                .unwrap()
                .unwrap();

        let request = parse_raw_request(&mut tcp_stream).await;

        tcp_stream
            .write_all(b"HTTP/1.1 204 No Content")
            .await
            .unwrap();
        assert_eq!(host_header_values(&request), [&destination_host], "Did not find correct host header. The value should be the proxy destination ({destination_host}), not the proxy itself ({proxy_host})");
    }

    #[tokio::test]
    async fn does_not_forward_host_header_for_websocket_requests() {
        let target_host = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = target_host.local_addr().unwrap();

        let proxy_port = start_server_port(target.port(), vec!["unused token"]);
        tokio::spawn(async move {
            connect_to_websocket_port(proxy_port).await;
        });

        let proxy_host = format!("127.0.0.1:{proxy_port}");
        let destination_host = format!("127.0.0.1:{}", target.port());

        let (mut tcp_stream, _) =
            tokio::time::timeout(Duration::from_secs(5), target_host.accept())
                .await
                .unwrap()
                .unwrap();

        let request = parse_raw_request(&mut tcp_stream).await;

        assert_eq!(host_header_values(&request), [&destination_host], "Did not find correct host header. The value should be the proxy destination ({destination_host}), not the proxy itself ({proxy_host})");
    }

    async fn parse_raw_request(tcp_stream: &mut TcpStream) -> httparse::Request<'static, 'static> {
        let mut incoming_payload = Vec::with_capacity(10000);
        tcp_stream.read_buf(&mut incoming_payload).await.unwrap();
        let headers = Vec::from([httparse::EMPTY_HEADER; 64]).leak();
        let mut request = httparse::Request::new(headers);
        request.parse(incoming_payload.leak()).unwrap();

        request
    }

    fn host_header_values<'a>(request: &httparse::Request<'a, '_>) -> Vec<&'a str> {
        request
            .headers
            .iter()
            .filter(|header| header.name.to_lowercase() == "host")
            .map(|header| std::str::from_utf8(header.value).unwrap())
            .collect::<Vec<_>>()
    }

    #[tokio::test]
    async fn forwards_websockets() {
        let (listener, port) = axum_server().await;
        let (test_app, _) = websocket_app(receive_all_messages);
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["unused token"]);

        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;
        ws.send(Message::Ping("test".as_bytes().into()))
            .await
            .expect("Error sending to websocket");

        assert_eq!(
            ws.next()
                .await
                .unwrap()
                .expect("Error receiving from websocket"),
            Message::Pong("test".as_bytes().into())
        );
    }

    #[tokio::test]
    async fn closes_remote_connection_when_local_client_disconnects_unexpectedly() {
        let (listener, port) = axum_server().await;
        let (test_app, connection_closed) = websocket_app(receive_all_messages);
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["unused token"]);

        let (ws, _) = connect_to_websocket_port(proxy_port).await;
        drop(ws);

        connection_closed.await.unwrap();
    }

    #[tokio::test]
    async fn closes_remote_connection_when_local_client_disconnects_gracefully() {
        let (listener, port) = axum_server().await;
        let (test_app, connection_closed) = websocket_app(receive_all_messages);
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["unused token"]);
        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;
        ws.close(None).await.unwrap();

        connection_closed.await.unwrap();
    }

    #[tokio::test]
    async fn closes_local_connection_when_remote_client_disconnects_gracefully() {
        let (listener, port) = axum_server().await;
        let (test_app, connection_closed) = websocket_app(close_connection);
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["unused token"]);
        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;

        connection_closed.await.unwrap();
        assert_eq!(timeout(ws.next()).await.unwrap().unwrap(), Close(None));
    }

    #[tokio::test]
    async fn closes_local_connection_gracefully_when_remote_does_not_respond() {
        let proxy_port = start_server_port(0, vec!["unused token"]);
        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;

        assert_eq!(
            timeout(ws.next()).await.unwrap().unwrap(),
            Close(Some(CloseFrame {
                code: CloseCode::Protocol,
                reason: "Protocol violation".into(),
            }))
        );
    }

    #[tokio::test]
    async fn closes_local_connection_when_remote_client_disconnects_forcefully() {
        let (test_app, _connection_closed) = websocket_app(drop_connection);
        let (listener, port) = axum_server().await;
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["unused token"]);
        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;

        assert_eq!(
            timeout(ws.next()).await.unwrap().unwrap(),
            Close(Some(CloseFrame {
                code: CloseCode::Protocol,
                reason: "Protocol violation".into(),
            }))
        );
    }

    async fn timeout<Fut: Future>(fut: Fut) -> Fut::Output {
        tokio::time::timeout(Duration::from_secs(5), fut)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn updates_outdated_jwts_for_websocket_connection() {
        let test_app = Router::new()
            .route(
                "/my/ws/endpoint",
                get(|ws: WebSocketUpgrade| async { ws.on_upgrade(|_ws| ready(())) }),
            )
            .layer(axum::middleware::from_fn(auth(|token| {
                token == "correct token"
            })));
        let (listener, port) = axum_server().await;
        tokio::spawn(axum::serve(listener, test_app.into_make_service()).into_future());
        let proxy_port = start_server_port(port, vec!["outdated token", "correct token"]);
        let (mut ws, _) = connect_to_websocket_port(proxy_port).await;
        ws.send(Message::Ping("test".as_bytes().into()))
            .await
            .expect("Error sending to websocket");
        assert_eq!(
            ws.next()
                .await
                .unwrap()
                .expect("Error receiving from websocket"),
            Message::Pong("test".as_bytes().into())
        );
    }

    async fn axum_server() -> (tokio::net::TcpListener, u16) {
        for port in 3200..3300 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);
            if let Ok(server) = tokio::net::TcpListener::bind(&addr).await {
                return (server, port);
            }
        }
        panic!("No free port found")
    }

    #[allow(clippy::type_complexity)]
    fn auth<'a>(
        token_is_valid: fn(&str) -> bool,
    ) -> impl Fn(
        TypedHeader<Authorization<Bearer>>,
        Request<axum::body::Body>,
        Next,
    ) -> BoxFuture<'a, Result<Response, StatusCode>>
           + Clone {
        move |TypedHeader(auth), request, next| {
            Box::pin(async move {
                if token_is_valid(auth.token()) {
                    let response = next.run(request).await;
                    Ok(response)
                } else {
                    Err(StatusCode::UNAUTHORIZED)
                }
            })
        }
    }

    async fn connect_to_websocket_port(
        port: u16,
    ) -> (
        WebSocketStream<MaybeTlsStream<TcpStream>>,
        Response<Option<Vec<u8>>>,
    ) {
        use rustls::*;
        #[derive(Debug)]
        struct CertificateIgnorer;
        impl client::danger::ServerCertVerifier for CertificateIgnorer {
            fn verify_server_cert(
                &self,
                _end_entity: &CertificateDer<'_>,
                _intermediates: &[CertificateDer<'_>],
                _server_name: &ServerName<'_>,
                _ocsp_response: &[u8],
                _now: UnixTime,
            ) -> Result<ServerCertVerified, Error> {
                Ok(ServerCertVerified::assertion())
            }

            fn verify_tls12_signature(
                &self,
                _message: &[u8],
                _cert: &rustls::pki_types::CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn verify_tls13_signature(
                &self,
                _message: &[u8],
                _cert: &rustls::pki_types::CertificateDer<'_>,
                _dss: &rustls::DigitallySignedStruct,
            ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
            {
                Ok(HandshakeSignatureValid::assertion())
            }

            fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
                vec![
                    rustls::SignatureScheme::RSA_PKCS1_SHA1,
                    rustls::SignatureScheme::ECDSA_SHA1_Legacy,
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                    rustls::SignatureScheme::RSA_PKCS1_SHA384,
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                    rustls::SignatureScheme::RSA_PKCS1_SHA512,
                    rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
                    rustls::SignatureScheme::RSA_PSS_SHA256,
                    rustls::SignatureScheme::RSA_PSS_SHA384,
                    rustls::SignatureScheme::RSA_PSS_SHA512,
                    rustls::SignatureScheme::ED25519,
                    rustls::SignatureScheme::ED448,
                ]
            }
        }

        let mut config = ClientConfig::builder()
            .with_root_certificates(Arc::new(RootCertStore::empty()))
            .with_no_client_auth();
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(CertificateIgnorer));
        tokio_tungstenite::connect_async_tls_with_config(
            format!("wss://127.0.0.1:{port}/c8y/my/ws/endpoint"),
            None,
            false,
            Some(Connector::Rustls(Arc::new(config))),
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn forwards_successful_responses() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(204)
            .create_async()
            .await;

        let port = start_server(&server, vec!["test-token"]);

        let res = reqwest_client()
            .get(format!("https://localhost:{port}/c8y/hello"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    }

    #[tokio::test]
    async fn uses_configured_server_certificate() {
        let _ = env_logger::try_init();

        let certificate = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
        let cert_der = certificate.serialize_der().unwrap();

        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(204)
            .create_async()
            .await;

        let port = start_server_with_certificate(&server, vec!["test-token"], certificate, None);

        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::builder()
            .add_root_certificate(reqwest::tls::Certificate::from_der(&cert_der).unwrap())
            .build()
            .unwrap();
        let res = client
            .get(format!("https://localhost:{port}/c8y/hello"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    }

    #[tokio::test]
    async fn forwards_unsuccessful_responses() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/not-a-known-url")
            .with_status(404)
            .create_async()
            .await;

        let port = start_server(&server, vec!["test-token"]);

        let res = reqwest_client()
            .get(format!("https://localhost:{port}/c8y/not-a-known-url"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    }

    #[tokio::test]
    async fn responds_with_bad_gateway_on_connection_error() {
        let _ = env_logger::try_init();

        let port = start_proxy_to_url(
            "127.0.0.1:0",
            vec!["test-token"],
            rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap(),
            None,
        );

        let res = reqwest_client()
            .get(format!("https://localhost:{port}/c8y/not-a-known-url"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 502);
    }

    #[tokio::test]
    async fn sends_query_string_from_original_request() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/inventory/managedObjects")
            .match_query("pageSize=100")
            .with_status(200)
            .create_async()
            .await;

        let port = start_server(&server, vec!["test-token"]);

        let res = reqwest_client()
            .get(format!(
                "https://localhost:{port}/c8y/inventory/managedObjects?pageSize=100"
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[tokio::test]
    async fn uses_authorization_header_passed_by_user_if_one_is_provided() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/inventory/managedObjects")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .create_async()
            .await;

        let port = start_server(&server, vec!["test-token"]);

        let res = reqwest_client()
            .get(format!(
                "https://localhost:{port}/c8y/inventory/managedObjects"
            ))
            .basic_auth("test", Some("test"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    }

    #[tokio::test]
    async fn retries_requests_with_small_bodies() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer old-token")
            .with_status(401)
            .create_async()
            .await;
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .match_body("A body")
            .with_body("Some response")
            .with_status(200)
            .create_async()
            .await;

        let port = start_server(&server, vec!["old-token", "test-token"]);

        let body = "A body";
        let res = reqwest_client()
            .put(format!("https://localhost:{port}/c8y/hello"))
            .header("Content-Length", body.bytes().len())
            .body(body)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200);
        assert_eq!(res.bytes().await.unwrap(), Bytes::from("Some response"));
    }

    #[tokio::test]
    async fn regenerates_token_proactively_if_the_request_cannot_be_retried() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        let head_request = server
            .mock("HEAD", "/tenant/currentTenant")
            .match_header("Authorization", "Bearer old-token")
            .with_status(401)
            .create_async()
            .await;
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .match_body("A body")
            .with_body("Some response")
            .with_status(200)
            .create_async()
            .await;

        let port = start_server(&server, vec!["old-token", "test-token"]);

        let body = "A body";
        let res = reqwest_client()
            .put(format!("https://localhost:{port}/c8y/hello"))
            .body(reqwest::Body::wrap_stream(once(ready(Ok::<
                _,
                std::convert::Infallible,
            >(body)))))
            .send()
            .await
            .unwrap();

        head_request.assert();
        assert_eq!(res.status(), 200);
        assert_eq!(res.bytes().await.unwrap(), Bytes::from("Some response"));
    }

    #[tokio::test]
    async fn retries_get_request_on_401() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer stale-token")
            .with_status(401)
            .with_body("Token expired")
            .create_async()
            .await;
        server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(200)
            .with_body("Succeeded")
            .create_async()
            .await;

        let port = start_server(&server, vec!["stale-token", "test-token"]);

        let res = reqwest_client()
            .get(format!("https://localhost:{port}/c8y/hello"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.bytes().await.unwrap(), Bytes::from("Succeeded"));
    }

    #[allow(clippy::disallowed_methods)]
    fn reqwest_client() -> reqwest::Client {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap()
    }

    fn start_server(server: &mockito::Server, tokens: Vec<impl Into<Cow<'static, str>>>) -> u16 {
        start_server_with_certificate(
            server,
            tokens,
            rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap(),
            None,
        )
    }

    fn start_server_port(port: u16, tokens: Vec<impl Into<Cow<'static, str>>>) -> u16 {
        start_proxy_to_url(
            &format!("127.0.0.1:{port}"),
            tokens,
            rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap(),
            None,
        )
    }

    fn start_server_with_certificate(
        target_host: &mockito::Server,
        tokens: Vec<impl Into<Cow<'static, str>>>,
        certificate: rcgen::Certificate,
        ca_dir: Option<Utf8PathBuf>,
    ) -> u16 {
        let url = target_host.url();
        let (_scheme, host) = url.split_once("://").unwrap();
        start_proxy_to_url(host, tokens, certificate, ca_dir)
    }

    #[allow(clippy::disallowed_methods)]
    fn start_proxy_to_url(
        target_host: &str,
        tokens: Vec<impl Into<Cow<'static, str>>>,
        certificate: rcgen::Certificate,
        ca_dir: Option<Utf8PathBuf>,
    ) -> u16 {
        let jwt_retriever = IterJwtRetriever::new(tokens).shared();
        let mut last_error = None;
        for port in 3000..3100 {
            let state = AppData {
                is_https: false,
                host: target_host.into(),
                token_manager: jwt_retriever.clone(),
                client: reqwest::Client::new(),
            };
            let trust_store = ca_dir
                .as_ref()
                .map(|dir| axum_tls::read_trust_store(dir).unwrap());
            let config = axum_tls::ssl_config(
                vec![certificate.serialize_der().unwrap().into()],
                PrivateKeyDer::from_pem_slice(certificate.serialize_private_key_pem().as_bytes())
                    .unwrap(),
                trust_store,
            )
            .unwrap();
            let app = create_app(state);
            let res = try_bind_with_tls(app, Ipv4Addr::LOCALHOST.into(), port, config);
            match res {
                Ok(server) => {
                    tokio::spawn(server);
                    return port;
                }
                Err(e) => last_error = Some(e),
            }
        }

        panic!("Failed to bind to any port: {}", last_error.unwrap());
    }

    /// A JwtRetriever that returns a sequence of JWT tokens
    pub(crate) struct IterJwtRetriever {
        tokens: std::vec::IntoIter<Cow<'static, str>>,
        cached: Option<Arc<str>>,
    }

    #[async_trait::async_trait]
    impl TokenManager for IterJwtRetriever {
        async fn refresh(&mut self) -> Result<Arc<str>, anyhow::Error> {
            let jwt: Arc<str> = format!("Bearer {}", self.tokens.next().unwrap()).into();
            self.cached = Some(jwt.clone());
            Ok(jwt)
        }

        fn cached_mut(&mut self) -> Option<&mut Arc<str>> {
            self.cached.as_mut()
        }
    }

    impl IterJwtRetriever {
        pub fn new(tokens: Vec<impl Into<Cow<'static, str>>>) -> Self {
            IterJwtRetriever {
                tokens: tokens
                    .into_iter()
                    .map(|token| token.into())
                    .collect::<Vec<_>>()
                    .into_iter(),
                cached: None,
            }
        }

        pub fn shared(self) -> SharedTokenManager {
            SharedTokenManager(Arc::new(Mutex::new(self)))
        }
    }
}
