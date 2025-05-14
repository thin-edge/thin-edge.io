use tokio::net::UnixListener;

use anyhow::Context;
use tracing::error;
use tracing::info;

use super::connection::Connection;
use crate::connection::Frame1;
use crate::connection::ProtocolError;
use crate::service::SigningService;

pub struct TedgeP11Server {
    service: Box<dyn SigningService + Send + Sync>,
}

impl TedgeP11Server {
    pub fn new<S>(service: S) -> anyhow::Result<Self>
    where
        S: SigningService + Send + Sync + 'static,
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
            | Frame1::SignResponse { .. } => {
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
        };

        connection.write_frame(&response).context("write")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::client::TedgeP11Client;
    use crate::service::*;
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    use super::*;

    const SCHEME: rustls::SignatureScheme = rustls::SignatureScheme::ECDSA_NISTP256_SHA256;
    const SIGNATURE: [u8; 2] = [0x21, 0x37];

    struct TestSigningService;

    impl SigningService for TestSigningService {
        fn choose_scheme(
            &self,
            _request: ChooseSchemeRequest,
        ) -> anyhow::Result<ChooseSchemeResponse> {
            Ok(ChooseSchemeResponse {
                scheme: Some(SignatureScheme(SCHEME)),
                algorithm: SignatureAlgorithm(rustls::SignatureAlgorithm::ECDSA),
            })
        }

        fn sign(&self, _request: SignRequest) -> anyhow::Result<SignResponse> {
            Ok(SignResponse(SIGNATURE.to_vec()))
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
            assert_eq!(client.choose_scheme(&[], None).unwrap().unwrap(), SCHEME);
            assert_eq!(&client.sign(&[], None).unwrap(), &SIGNATURE[..]);
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
