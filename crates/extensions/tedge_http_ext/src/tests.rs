use crate::*;
use backoff::exponential::ExponentialBackoff;
use http::StatusCode;
use rustls::ClientConfig;
use rustls::RootCertStore;
use tedge_actors::ClientMessageBox;

#[tokio::test]
async fn get_over_https() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server.mock("GET", "/").create_async().await;

    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get(server.url())
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
}

#[tokio::test]
async fn requests_include_thin_edge_user_agent() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .match_header("user-agent", certificate::http_client::USER_AGENT)
        .with_status(200)
        .create_async()
        .await;

    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get(server.url())
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.expect("some response");
    assert!(response.is_ok());
    assert_eq!(response.unwrap().status(), 200);
    _mock.assert();
}

#[tokio::test]
async fn retries_on_502() {
    let mut server = mockito::Server::new_async().await;
    let _mock = server
        .mock("GET", "/")
        .with_status(502)
        .expect(2)
        .create_async()
        .await;

    let mut http = spawn_http_actor().await;

    let request = HttpRequestBuilder::get(server.url())
        .build()
        .expect("A simple HTTPS GET request");

    let response = http.await_response(request).await.unwrap();
    assert!(matches!(
        response.unwrap_err(),
        HttpError::HttpStatusError {
            code: StatusCode::BAD_GATEWAY,
            ..
        }
    ));
    _mock.assert();
}

async fn spawn_http_actor() -> ClientMessageBox<HttpRequest, HttpResult> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let config = ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    let mut builder = HttpActor::new(config)
        .with_backoff(
            // for test: tweaked to exactly 1 retry
            ExponentialBackoff {
                initial_interval: Duration::from_millis(5),
                multiplier: 10.0,
                randomization_factor: f64::EPSILON,
                max_elapsed_time: Some(Duration::from_millis(50)),
                ..Default::default()
            },
        )
        .builder();
    let handle = ClientMessageBox::new(&mut builder);

    tokio::spawn(builder.run());

    handle
}
