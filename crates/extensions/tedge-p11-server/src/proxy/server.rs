use tokio::net::UnixListener;

use anyhow::Context;
use tracing::error;
use tracing::info;

use super::connection::Connection;
use super::connection::Frame1;
use super::connection::ProtocolError;
use crate::proxy::request::Request;
use crate::service::SignRequestWithSigScheme;
use crate::service::TedgeP11Service;

/// Relays requests made by [`TedgeP11Client`](super::TedgeP11Client) to the inner PKCS #11 service and returns
/// responses.
pub struct TedgeP11Server {
    service: Box<dyn TedgeP11Service>,
}

impl TedgeP11Server {
    pub fn new<S>(service: S) -> anyhow::Result<Self>
    where
        S: TedgeP11Service + 'static,
    {
        Ok(Self {
            service: Box::new(service),
        })
    }

    /// Handle multiple requests on a given listener.
    pub async fn serve(&self, listener: UnixListener) -> anyhow::Result<()> {
        // Accept a connection
        loop {
            let (stream, _) = listener
                .accept()
                .await
                .context("Failed to accept connection")?;

            let stream = stream.into_std()?;
            stream
                .set_nonblocking(false)
                .context("Failed to set nonblocking=false")?;
            let connection = Connection::new(stream);

            match self.process(connection) {
                Ok(_) => info!("Incoming request successful"),
                Err(e) => error!("Incoming request failed: {e:?}"),
            }
        }
    }

    fn process(&self, mut connection: Connection) -> anyhow::Result<()> {
        let request = connection
            .read_frame()
            .context("read")
            .and_then(|f| {
                Request::try_from(f)
                    .ok()
                    .ok_or(anyhow::anyhow!("frame is not a request"))
            })
            .context("invalid request");
        let request = match request {
            Ok(request) => request,
            Err(err) => {
                let _ = connection.write_frame(&Frame1::Error(ProtocolError(format!("{err:#}"))));
                return Err(err);
            }
        };

        // server should read request and respond with response, and connection layer should map to correct frame
        let response = match request {
            Request::ChooseSchemeRequest(request) => self
                .service
                .choose_scheme(request)
                .map(Frame1::ChooseSchemeResponse),

            Request::SignRequest(request) => {
                let sign_request_2 = SignRequestWithSigScheme {
                    to_sign: request.to_sign,
                    uri: request.uri,
                    sigscheme: None,
                    pin: request.pin,
                };
                self.service.sign(sign_request_2).map(Frame1::SignResponse)
            }

            Request::SignRequestWithSigScheme(request) => {
                self.service.sign(request).map(Frame1::SignResponse)
            }

            Request::GetPublicKeyPemRequest(uri) => self
                .service
                .get_public_key_pem(uri.as_deref())
                .map(Frame1::GetPublicKeyPemResponse),

            // The Ping/Pong request does no PKCS11/cryptographic operations and is there only so a
            // client can confirm that tedge-p11-server is running and is ready to serve requests.
            // Notably, with systemd being configured to start the service when a request is
            // received on the associated socket, a Ping/Pong request triggers a service start and
            // ensures the PKCS11 library is loaded and ready to serve signing requests. In
            // practice, this only occurs with a client calls TedgeP11Client::with_ready_check.
            Request::Ping => Ok(Frame1::Pong(None)),

            Request::CreateKeyRequest(request) => self
                .service
                .create_key(request)
                .map(Frame1::CreateKeyResponse),

            Request::GetTokensUrisRequest => self
                .service
                .get_tokens_uris()
                .map(Frame1::GetTokensUrisResponse),
        };

        match response {
            Ok(response) => connection
                .write_frame(&response)
                .context("failed to write response")?,
            Err(err) => {
                let response =
                    Frame1::Error(ProtocolError(format!("PKCS #11 service failed: {err:#}")));
                connection
                    .write_frame(&response)
                    .context("failed to write response")?;
                anyhow::bail!(err);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;

    use super::super::client::TedgeP11Client;
    use crate::pkcs11;
    use crate::proxy::frame::Frame;
    use crate::service::*;
    use std::time::Duration;

    const SCHEME: pkcs11::SigScheme = pkcs11::SigScheme::EcdsaNistp256Sha256;
    const SIGNATURE: [u8; 2] = [0x21, 0x37];

    struct TestSigningService;

    impl TedgeP11Service for TestSigningService {
        fn choose_scheme(
            &self,
            _request: ChooseSchemeRequest,
        ) -> anyhow::Result<ChooseSchemeResponse> {
            Ok(ChooseSchemeResponse {
                scheme: Some(SignatureScheme(SCHEME.into())),
                algorithm: SignatureAlgorithm(rustls::SignatureAlgorithm::ECDSA),
            })
        }

        fn sign(&self, _request: SignRequestWithSigScheme) -> anyhow::Result<SignResponse> {
            Ok(SignResponse(SIGNATURE.to_vec()))
        }

        fn get_public_key_pem(&self, _uri: Option<&str>) -> anyhow::Result<String> {
            todo!()
        }

        fn create_key(&self, _request: CreateKeyRequest) -> anyhow::Result<CreateKeyResponse> {
            todo!()
        }

        fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
            todo!()
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

        tokio::task::spawn_blocking(move || {
            let client = TedgeP11Client::with_ready_check(socket_path.into());
            assert_eq!(
                client.choose_scheme(&[], None).unwrap().scheme.unwrap(),
                SCHEME.into()
            );
            assert_eq!(&client.sign2(&[], None, SCHEME).unwrap(), &SIGNATURE[..]);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn server_responds_with_error_to_invalid_request() {
        let (client, _s) = setup_test().await;
        let s = client.0.into_std().unwrap();
        s.set_nonblocking(false).unwrap();

        let response = tokio::task::spawn_blocking(move || {
            let mut client_connection = Connection::new(s);
            client_connection
                .write_frame(&Frame1::SignResponse(SignResponse(vec![])))
                .unwrap();
            client_connection.read_frame().unwrap()
        })
        .await
        .unwrap();
        assert!(matches!(response, Frame::Version1(Frame1::Error(_))));
    }

    #[tokio::test]
    async fn server_responds_with_error_to_garbage() {
        let (mut client, _) = setup_test().await;

        client.write_and_close("garbage".as_bytes()).await;
        let response = client.read().await;

        let response: Frame = postcard::from_bytes(&response).unwrap();
        let Frame::Version1(Frame1::Error(ProtocolError(err_msg))) = response else {
            panic!("should be error");
        };

        assert!(err_msg.contains("invalid request"));
    }

    #[tokio::test]
    async fn server_reports_invalid_commands() {
        let (mut client, _) = setup_test().await;

        let mut command = r#"{"NonexistingCommand":{}}"#.as_bytes().to_vec();
        // frame version2
        command.insert(0, 1);

        client.write_and_close(&command).await;
        let response = client.read().await;

        let response: Frame = postcard::from_bytes(&response).unwrap();
        let Frame::Version1(Frame1::Error(ProtocolError(err_msg))) = response else {
            panic!("should be error");
        };

        assert!(err_msg.contains(
            "invalid request: read: unknown variant `NonexistingCommand`, expected one of"
        ));
    }

    #[tokio::test]
    async fn server_reports_invalid_arguments() {
        let (mut client, _) = setup_test().await;

        let mut command = r#"{"SignRequest":{"message": [1, 2, 3]}}"#.as_bytes().to_vec();
        // frame version2
        command.insert(0, 1);

        client.write_and_close(&command).await;
        let response = client.read().await;

        let response: Frame = postcard::from_bytes(&response).unwrap();
        let Frame::Version1(Frame1::Error(ProtocolError(err_msg))) = response else {
            panic!("should be error");
        };

        assert!(err_msg.contains("missing field `to_sign` at line 1 column 37"));
    }

    async fn setup_test() -> (TestClient, tokio::task::JoinHandle<anyhow::Result<()>>) {
        let server = TedgeP11Server::new(TestSigningService).unwrap();
        let tmpdir = tempfile::tempdir().unwrap();
        let socket_path = tmpdir.path().join("test_socket.sock");
        dbg!(&socket_path);
        let listener = UnixListener::bind(&socket_path).unwrap();

        let server = tokio::spawn(async move { server.serve(listener).await });
        // wait until the server calls accept()
        tokio::time::sleep(Duration::from_millis(2)).await;

        let client_socket = TestClient(tokio::net::UnixStream::connect(socket_path).await.unwrap());
        (client_socket, server)
    }

    struct TestClient(tokio::net::UnixStream);

    impl TestClient {
        async fn write_and_close(&mut self, bytes: &[u8]) {
            self.0.write_all(bytes).await.unwrap();
            self.0.flush().await.unwrap();
            self.0.shutdown().await.unwrap();
        }

        async fn read(&mut self) -> Vec<u8> {
            let mut bytes = Vec::new();
            self.0.read_to_end(&mut bytes).await.unwrap();
            bytes
        }
    }
}
