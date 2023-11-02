use crate::tls::get_ssl_config;
use crate::tokens::*;
use axum::body::Body;
use axum::body::BoxBody;
use axum::body::Full;
use axum::body::StreamBody;
use axum::extract::FromRef;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use camino::Utf8PathBuf;
use futures::future::BoxFuture;
use futures::FutureExt;
use hyper::HeaderMap;
use miette::Context;
use miette::IntoDiagnostic;
use reqwest::Method;
use reqwest::StatusCode;
use std::fmt;
use std::future::Future;
use std::net::IpAddr;
use std::net::TcpListener;
use std::sync::Arc;
use tracing::error;
use tracing::info;

pub struct Server {
    fut: BoxFuture<'static, std::io::Result<()>>,
}

impl Server {
    pub(crate) fn try_init(
        state: AppState,
        address: IpAddr,
        port: u16,
        certificate_der: Vec<Vec<u8>>,
        key_der: Vec<u8>,
        ca_dir: Option<Utf8PathBuf>,
    ) -> miette::Result<Self> {
        Ok(Server {
            fut: try_run_server(address, port, state, certificate_der, key_der, ca_dir)?.boxed(),
        })
    }

    pub fn wait(self) -> BoxFuture<'static, std::io::Result<()>> {
        self.fut
    }
}

struct ProxyError(miette::Report);

impl From<miette::Report> for ProxyError {
    fn from(value: miette::Report) -> Self {
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

fn try_run_server(
    address: IpAddr,
    port: u16,
    state: AppState,
    certificate_der: Vec<Vec<u8>>,
    key_der: Vec<u8>,
    ca_dir: Option<Utf8PathBuf>,
) -> miette::Result<impl Future<Output = std::io::Result<()>>> {
    info!("Launching on port {port}");
    let handle = get(respond_to)
        .post(respond_to)
        .put(respond_to)
        .patch(respond_to)
        .delete(respond_to)
        .options(respond_to);
    let app = Router::new()
        .route("/c8y", handle.clone())
        .route("/c8y/", handle.clone())
        .route("/c8y/*path", handle)
        .with_state(state);
    let listener = TcpListener::bind((address, port))
        .into_diagnostic()
        .wrap_err_with(|| format!("binding to port {port}"))?;
    let config = get_ssl_config(certificate_der, key_der, ca_dir)?;
    Ok(axum_server::from_tcp(listener)
        .acceptor(crate::tls::Acceptor::new(config))
        .serve(app.into_make_service()))
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub target_host: Arc<str>,
    pub token_manager: SharedTokenManager,
}

impl FromRef<AppState> for TargetHost {
    fn from_ref(input: &AppState) -> Self {
        Self(input.target_host.clone())
    }
}

impl FromRef<AppState> for SharedTokenManager {
    fn from_ref(input: &AppState) -> Self {
        input.token_manager.clone()
    }
}

#[derive(Clone)]
struct TargetHost(Arc<str>);

impl fmt::Display for TargetHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

async fn respond_to(
    State(TargetHost(host)): State<TargetHost>,
    retrieve_token: State<SharedTokenManager>,
    path: Option<Path<String>>,
    uri: hyper::Uri,
    method: Method,
    headers: HeaderMap<HeaderValue>,
    small_body: crate::body::PossiblySmallBody,
) -> Result<(StatusCode, Option<HeaderMap>, BoxBody), ProxyError> {
    let path = match &path {
        Some(Path(p)) => p.as_str(),
        None => "",
    };
    let auth: fn(reqwest::RequestBuilder, &str) -> reqwest::RequestBuilder =
        if headers.contains_key("Authorization") {
            |req, _token| req
        } else {
            |req, token| req.bearer_auth(token)
        };

    // Cumulocity revokes the device token if we access parts of the frontend UI,
    // so deny requests to these proactively
    if path.ends_with(".js") || path.starts_with("apps/") {
        return Ok((StatusCode::FORBIDDEN, None, <_>::default()));
    }
    let mut destination = format!("{host}/{path}");
    if let Some(query) = uri.query() {
        destination += "?";
        destination += query;
    }

    let mut token = retrieve_token.not_matching(None).await;

    let client = reqwest::Client::new();
    let (body, body_clone) = small_body.try_clone();
    if body_clone.is_none() {
        let destination = format!("{host}/tenant/currentTenant");
        let response = client
            .head(&destination)
            .bearer_auth(&token)
            .send()
            .await
            .into_diagnostic()
            .wrap_err_with(|| format!("making HEAD request to {destination}"))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            token = retrieve_token.not_matching(Some(&token)).await;
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
    let mut res = send_request(body, &token)
        .await
        .into_diagnostic()
        .wrap_err_with(|| format!("making proxied request to {destination}"))?;

    if res.status() == StatusCode::UNAUTHORIZED {
        token = retrieve_token.not_matching(Some(&token)).await;
        if let Some(body) = body_clone {
            res = send_request(Body::from(body), &token)
                .await
                .into_diagnostic()
                .wrap_err_with(|| format!("making proxied request to {destination}"))?;
        }
    }
    let te_header = res.headers_mut().remove("transfer-encoding");
    let status = res.status();
    let headers = std::mem::take(res.headers_mut());

    let body = if te_header.map_or(false, |h| {
        h.to_str().unwrap_or_default().contains("chunked")
    }) {
        axum::body::boxed(StreamBody::new(res.bytes_stream()))
    } else {
        axum::body::boxed(Full::new(
            res.bytes()
                .await
                .into_diagnostic()
                .wrap_err("reading proxy response bytes")?,
        ))
    };

    Ok((status, Some(headers), body))
}

#[cfg(test)]
mod tests {
    use axum::async_trait;
    use axum::body::Bytes;
    use c8y_http_proxy::credentials::JwtRequest;
    use c8y_http_proxy::credentials::JwtResult;
    use c8y_http_proxy::credentials::JwtRetriever;
    use reqwest::Identity;
    use std::borrow::Cow;
    use std::error::Error;
    use std::net::Ipv4Addr;
    use tedge_actors::Sequential;
    use tedge_actors::Server;
    use tedge_actors::ServerActorBuilder;
    use tedge_actors::ServerConfig;

    use super::*;

    #[tokio::test]
    async fn forwards_successful_responses() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(204)
            .create();

        let port = start_server(&server, vec!["test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
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
    async fn uses_configured_server_certificate() {
        let _ = env_logger::try_init();

        let certificate = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
        let cert_der = certificate.serialize_der().unwrap();

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(204)
            .create();

        let port = start_server_with_certificate(&server, vec!["test-token"], certificate, None);

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
    async fn rejects_unknown_client_certificates() {
        let _ = env_logger::try_init();

        let certificate = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
        let cert_der = certificate.serialize_der().unwrap();

        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(204)
            .create();

        std::fs::create_dir_all("/tmp/test").unwrap();
        let port = start_server_with_certificate(
            &server,
            vec!["test-token"],
            certificate,
            Some("/tmp/test".into()),
        );

        let client_cert = rcgen::generate_simple_self_signed(["not-authorized".into()]).unwrap();
        let mut pem = client_cert.serialize_private_key_pem().into_bytes();
        pem.append(&mut client_cert.serialize_pem().unwrap().into_bytes());
        let identity = Identity::from_pem(&pem).unwrap();
        let client = reqwest::Client::builder()
            .add_root_certificate(reqwest::tls::Certificate::from_der(&cert_der).unwrap())
            .identity(identity)
            .build()
            .unwrap();
        let err = client
            .get(format!("https://localhost:{port}/c8y/hello"))
            .send()
            .await
            .unwrap_err();
        assert_matches::assert_matches!(
            rustls_error_from_reqwest(&err),
            rustls::Error::AlertReceived(rustls::AlertDescription::UnknownCA)
        );
    }

    #[tokio::test]
    async fn forwards_unsuccessful_responses() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/not-a-known-url")
            .with_status(404)
            .create();

        let port = start_server(&server, vec!["test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let res = client
            .get(format!("https://localhost:{port}/c8y/not-a-known-url"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    }

    #[tokio::test]
    async fn responds_with_bad_gateway_on_connection_error() {
        let _ = env_logger::try_init();

        // TODO worth introducing a builder here?
        let port = start_server_at_url(
            Arc::from("127.0.0.1:0"),
            vec!["test-token"],
            rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap(),
            None,
        );

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let res = client
            .get(format!("https://localhost:{port}/c8y/not-a-known-url"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 502);
    }

    #[tokio::test]
    async fn sends_query_string_from_original_request() {
        let _ = env_logger::try_init();
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/inventory/managedObjects")
            .match_query("pageSize=100")
            .with_status(200)
            .create();

        let port = start_server(&server, vec!["test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let res = client
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
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("GET", "/inventory/managedObjects")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .create();

        let port = start_server(&server, vec!["test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let res = client
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
        let mut server = mockito::Server::new();
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer old-token")
            .with_status(401)
            .create();
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .match_body("A body")
            .with_body("Some response")
            .with_status(200)
            .create();

        let port = start_server(&server, vec!["old-token", "test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let body = "A body";
        let res = client
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
        let mut server = mockito::Server::new();
        let head_request = server
            .mock("HEAD", "/tenant/currentTenant")
            .match_header("Authorization", "Bearer old-token")
            .with_status(401)
            .create();
        let _mock = server
            .mock("PUT", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .match_body("A body")
            .with_body("Some response")
            .with_status(200)
            .create();

        let port = start_server(&server, vec!["old-token", "test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let body = "A body";
        let res = client
            .put(format!("https://localhost:{port}/c8y/hello"))
            .body(reqwest::Body::wrap_stream(futures::stream::once(
                futures::future::ready(Ok::<_, std::convert::Infallible>(body)),
            )))
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
        let mut server = mockito::Server::new();
        server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer stale-token")
            .with_status(401)
            .with_body("Token expired")
            .create();
        server
            .mock("GET", "/hello")
            .match_header("Authorization", "Bearer test-token")
            .with_status(200)
            .with_body("Succeeded")
            .create();

        let port = start_server(&server, vec!["stale-token", "test-token"]);

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap();
        let res = client
            .get(format!("https://localhost:{port}/c8y/hello"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.bytes().await.unwrap(), Bytes::from("Succeeded"));
    }

    fn start_server(server: &mockito::Server, tokens: Vec<impl Into<Cow<'static, str>>>) -> u16 {
        start_server_at_url(
            server.url().into(),
            tokens,
            rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap(),
            None,
        )
    }

    fn start_server_with_certificate(
        server: &mockito::Server,
        tokens: Vec<impl Into<Cow<'static, str>>>,
        certificate: rcgen::Certificate,
        ca_dir: Option<Utf8PathBuf>,
    ) -> u16 {
        start_server_at_url(server.url().into(), tokens, certificate, ca_dir)
    }

    fn start_server_at_url(
        target_host: Arc<str>,
        tokens: Vec<impl Into<Cow<'static, str>>>,
        certificate: rcgen::Certificate,
        ca_dir: Option<Utf8PathBuf>,
    ) -> u16 {
        let mut retriever = IterJwtRetriever::builder(tokens);
        let mut last_error = None;
        for port in 3000..3100 {
            let state = AppState {
                target_host: target_host.clone(),
                token_manager: TokenManager::new(JwtRetriever::new("TEST => JWT", &mut retriever))
                    .shared(),
            };
            let res = try_run_server(
                Ipv4Addr::LOCALHOST.into(),
                port,
                state,
                vec![certificate.serialize_der().unwrap()],
                certificate.serialize_private_key_der(),
                ca_dir.clone(),
            );
            match res {
                Ok(server) => {
                    tokio::spawn(server);
                    tokio::spawn(retriever.run());
                    return port;
                }
                Err(e) => last_error = Some(e),
            }
        }

        panic!("Failed to bind to any port: {}", last_error.unwrap());
    }

    fn rustls_error_from_reqwest(err: &reqwest::Error) -> &rustls::Error {
        err.source()
            .unwrap()
            .downcast_ref::<hyper::Error>()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .get_ref()
            .unwrap()
            .downcast_ref::<rustls::Error>()
            .unwrap()
    }

    /// A JwtRetriever that returns a sequence of JWT tokens
    pub(crate) struct IterJwtRetriever {
        pub tokens: std::vec::IntoIter<Cow<'static, str>>,
    }

    #[async_trait]
    impl Server for IterJwtRetriever {
        type Request = JwtRequest;
        type Response = JwtResult;

        fn name(&self) -> &str {
            "IterJwtRetriever"
        }

        async fn handle(&mut self, _request: Self::Request) -> Self::Response {
            Ok(self.tokens.next().unwrap().into())
        }
    }

    impl IterJwtRetriever {
        pub fn builder(
            tokens: Vec<impl Into<Cow<'static, str>>>,
        ) -> ServerActorBuilder<IterJwtRetriever, Sequential> {
            let server = IterJwtRetriever {
                tokens: tokens
                    .into_iter()
                    .map(|token| token.into())
                    .collect::<Vec<_>>()
                    .into_iter(),
            };
            ServerActorBuilder::new(server, &ServerConfig::default(), Sequential)
        }
    }
}
