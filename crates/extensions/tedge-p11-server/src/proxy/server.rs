use tokio::net::UnixListener;

use anyhow::Context;
use tracing::error;
use tracing::info;

use super::connection::Connection;
use super::connection::Frame1;
use super::connection::ProtocolError;
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
        let request = connection.read_frame().context("read")?;

        let response = match request {
            Frame1::Error(_)
            | Frame1::ChooseSchemeResponse { .. }
            | Frame1::SignResponse { .. }
            | Frame1::GetPublicKeyPemResponse(_)
            | Frame1::Pong
            | Frame1::CreateKeyResponse { .. }
            | Frame1::GetTokensUrisResponse(_) => {
                let error = ProtocolError("invalid request".to_string());
                let _ = connection.write_frame(&Frame1::Error(error));
                anyhow::bail!("protocol error: invalid request")
            }
            Frame1::ChooseSchemeRequest(request) => {
                let response = self.service.choose_scheme(request);
                match response {
                    Ok(response) => Frame1::ChooseSchemeResponse(response),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }
            Frame1::SignRequest(request) => {
                let sign_request_2 = SignRequestWithSigScheme {
                    to_sign: request.to_sign,
                    uri: request.uri,
                    sigscheme: None,
                    pin: request.pin,
                };
                let response = self.service.sign(sign_request_2);
                match response {
                    Ok(response) => Frame1::SignResponse(response),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }
            Frame1::SignRequestWithSigScheme(request) => {
                let response = self.service.sign(request);
                match response {
                    Ok(response) => Frame1::SignResponse(response),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }
            Frame1::GetPublicKeyPemRequest(uri) => {
                let response = self.service.get_public_key_pem(uri.as_deref());
                match response {
                    Ok(pubkey_pem) => Frame1::GetPublicKeyPemResponse(pubkey_pem),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }

            // The Ping/Pong request does no PKCS11/cryptographic operations and is there only so a
            // client can confirm that tedge-p11-server is running and is ready to serve requests.
            // Notably, with systemd being configured to start the service when a request is
            // received on the associated socket, a Ping/Pong request triggers a service start and
            // ensures the PKCS11 library is loaded and ready to serve signing requests. In
            // practice, this only occurs with a client calls TedgeP11Client::with_ready_check.
            Frame1::Ping => Frame1::Pong,

            Frame1::CreateKeyRequest(request) => {
                let response = self.service.create_key(request);
                match response {
                    Ok(pubkey_der) => Frame1::CreateKeyResponse(pubkey_der),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }

            Frame1::GetTokensUrisRequest => {
                let response = self.service.get_tokens_uris();
                match response {
                    Ok(response) => Frame1::GetTokensUrisResponse(response),
                    Err(err) => {
                        let response = Frame1::Error(ProtocolError(format!(
                            "PKCS #11 service failed: {err:#}"
                        )));
                        connection.write_frame(&response)?;
                        anyhow::bail!(err);
                    }
                }
            }
        };

        connection.write_frame(&response).context("write")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::client::TedgeP11Client;
    use crate::pkcs11;
    use crate::service::*;
    use std::io::Read;
    use std::os::unix::net::UnixStream;
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
        let service = TestSigningService;
        let server = TedgeP11Server::new(service).unwrap();
        let tmpdir = tempfile::tempdir().unwrap();
        let socket_path = tmpdir.path().join("test_socket.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();

        tokio::spawn(async move { server.serve(listener).await });
        // wait until the server calls accept()
        tokio::time::sleep(Duration::from_millis(2)).await;

        let response = tokio::task::spawn_blocking(move || {
            let mut client_connection = Connection::new(UnixStream::connect(socket_path).unwrap());
            client_connection
                .write_frame(&Frame1::SignResponse(SignResponse(vec![])))
                .unwrap();
            client_connection.read_frame().unwrap()
        })
        .await
        .unwrap();
        assert!(matches!(response, Frame1::Error(_)));
    }

    #[tokio::test]
    async fn server_responds_with_error_to_garbage() {
        use std::io::Write as _;

        let service = TestSigningService;
        let server = TedgeP11Server::new(service).unwrap();
        let tmpdir = tempfile::tempdir().unwrap();
        let socket_path = tmpdir.path().join("test_socket.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();

        tokio::spawn(async move { server.serve(listener).await });
        // wait until the server calls accept()
        tokio::time::sleep(Duration::from_millis(2)).await;

        // the reader should exit
        tokio::task::spawn_blocking(move || {
            let mut stream = UnixStream::connect(socket_path).unwrap();
            write!(stream, "garbage").unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        })
        .await
        .unwrap();
    }
}
