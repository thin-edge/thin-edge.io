use std::time::Duration;

use tokio::net::UnixListener;

use tedge_p11_server::client::TedgeP11Client;
use tedge_p11_server::service::ChooseSchemeRequest;
use tedge_p11_server::service::ChooseSchemeResponse;
use tedge_p11_server::service::SignRequest;
use tedge_p11_server::service::SignResponse;
use tedge_p11_server::service::SigningService;
use tedge_p11_server::TedgeP11Server;

struct TestSigningService;

impl SigningService for TestSigningService {
    fn choose_scheme(&self, _request: ChooseSchemeRequest) -> ChooseSchemeResponse {
        ChooseSchemeResponse {
            scheme: None,
            algorithm: tedge_p11_server::service::SignatureAlgorithm(
                rustls::SignatureAlgorithm::ECDSA,
            ),
        }
    }

    fn sign(&self, _request: SignRequest) -> SignResponse {
        SignResponse(vec![])
    }
}

/// Check that client successfully receives responses from the server about the requests. Tests the
/// connection, framing, serialization, but not PKCS#11 layer itself.
#[tokio::test]
async fn server_works_with_client() {
    let service = TestSigningService;
    let server = TedgeP11Server::new(service).unwrap();
    let tmpdir = tempfile::tempdir().unwrap();
    let socket_path = tmpdir.path().join("test_socket.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    tokio::spawn(async move { server.serve(listener).await });
    // wait until the server calls accept()
    tokio::time::sleep(Duration::from_millis(2)).await;

    let client = TedgeP11Client {
        socket_path: socket_path.into(),
    };

    tokio::task::spawn_blocking(move || {
        client.choose_scheme(&[]).unwrap();
        client.sign(&[]).unwrap();
    })
    .await
    .unwrap();
}
