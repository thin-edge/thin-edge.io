use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Context;
use tracing::debug;
use tracing::trace;

use super::connection::Connection;
use super::connection::Frame1;
use crate::pkcs11::SigScheme;
use crate::service::ChooseSchemeRequest;
use crate::service::ChooseSchemeResponse;
use crate::service::SignRequest;
use crate::service::SignRequestWithSigScheme;
use crate::service::TedgeP11Service;

/// A [`TedgeP11Service`] implementation that proxies requests to the [`TedgeP11Server`](super::TedgeP11Server) that
/// does the operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11Client {
    pub(crate) socket_path: Arc<Path>,
    pub(crate) uri: Option<Arc<str>>,
}

impl TedgeP11Service for TedgeP11Client {
    fn choose_scheme(
        &self,
        request: ChooseSchemeRequest,
    ) -> anyhow::Result<crate::service::ChooseSchemeResponse> {
        let offered: Vec<_> = request.offered.iter().map(|s| s.0).collect();
        self.choose_scheme(&offered, request.uri)
    }

    fn sign(
        &self,
        request: SignRequestWithSigScheme,
    ) -> anyhow::Result<crate::service::SignResponse> {
        let response = match request.sigscheme {
            Some(sigscheme) => self.sign2(&request.to_sign, request.uri, sigscheme)?,
            None => self.sign(&request.to_sign, request.uri)?,
        };

        Ok(crate::service::SignResponse(response))
    }

    fn get_public_key_pem(&self, uri: Option<&str>) -> anyhow::Result<String> {
        self.get_public_key_pem(uri.map(ToString::to_string))
    }
}

impl TedgeP11Client {
    /// Returns the client after performing a ready check to the server.
    ///
    /// Before returning the client, a request is made to the server to "warm it up", discarding the
    /// response.
    ///
    /// In some environments, starting up `tedge-p11-server` can take an unreasonable amount of
    /// time, which is a problem because if we wait for it to start up while we're making a TLS 1.3
    /// handshake, the TCP connection can get dropped due to hitting a timeout. What's worse, the
    /// application layer protocol (MQTT) doesn't seem to send a required [TLS `close_notify`
    /// message before EOF][unexp-eof], which results in an additional error from rustls:
    ///
    /// > peer closed connection without sending TLS close_notify:
    /// > https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
    ///
    /// To remedy this for now, we're making this additional request when creating the client to
    /// hopefully make the server the most ready it can be for handling the real requests.
    ///
    /// This is not ideal because the server can still get restarted between creating the client and
    /// making the sign request, but we're making a best-effort here not to add too much complexity
    /// to cater to environments which are not very sane and should be relatively rare.
    ///
    /// [unexp-eof]: https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
    pub fn with_ready_check(socket_path: Arc<Path>) -> Self {
        let client = Self {
            socket_path,
            uri: None,
        };

        // make any request to make sure the service is online and it will respond
        let _ = client.choose_scheme(&[], None);

        client
    }

    pub fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
        uri: Option<String>,
    ) -> anyhow::Result<ChooseSchemeResponse> {
        let request = Frame1::ChooseSchemeRequest(ChooseSchemeRequest {
            offered: offered
                .iter()
                .copied()
                .map(crate::service::SignatureScheme)
                .collect::<Vec<_>>(),
            uri,
        });
        let response = self.do_request(request)?;

        let Frame1::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme, received: {response:?}");
        };

        debug!("Choose scheme complete");

        Ok(response)
    }

    // this function is called only on the server when handling ClientHello message, so
    // realistically it won't ever be called in our case
    pub fn algorithm(&self) -> anyhow::Result<rustls::SignatureAlgorithm> {
        // if passed empty set of schemes, service doesn't return a scheme but returns an algorithm
        let request = Frame1::ChooseSchemeRequest(ChooseSchemeRequest {
            offered: vec![],
            uri: None,
        });
        let response = self.do_request(request)?;

        let Frame1::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme, received: {response:?}");
        };

        debug!("Choose scheme complete");

        Ok(response.algorithm.0)
    }

    pub fn sign(&self, message: &[u8], uri: Option<String>) -> anyhow::Result<Vec<u8>> {
        let request = Frame1::SignRequest(SignRequest {
            to_sign: message.to_vec(),
            uri,
        });
        let response = self.do_request(request)?;

        let Frame1::SignResponse(response) = response else {
            bail!("protocol error: bad response, expected sign, received: {response:?}");
        };

        debug!("Sign complete");

        Ok(response.0)
    }

    pub fn sign2(
        &self,
        message: &[u8],
        uri: Option<String>,
        sigscheme: SigScheme,
    ) -> anyhow::Result<Vec<u8>> {
        let request = Frame1::SignRequestWithSigScheme(SignRequestWithSigScheme {
            to_sign: message.to_vec(),
            sigscheme: Some(sigscheme),
            uri,
        });
        let response = self.do_request(request)?;

        let Frame1::SignResponse(response) = response else {
            bail!("protocol error: bad response, expected sign, received: {response:?}");
        };

        debug!("Sign complete");

        Ok(response.0)
    }

    pub fn get_public_key_pem(&self, uri: Option<String>) -> anyhow::Result<String> {
        let request = Frame1::GetPublicKeyPemRequest(uri);
        let response = self.do_request(request)?;

        let Frame1::GetPublicKeyPemResponse(pubkey_pem) = response else {
            bail!("protocol error: bad response, expected create_key, received: {response:?}");
        };

        debug!("Sign complete");

        Ok(pubkey_pem)
    }

    fn do_request(&self, request: Frame1) -> anyhow::Result<Frame1> {
        let stream = UnixStream::connect(&self.socket_path).with_context(|| {
            format!(
                "Failed to connect to tedge-p11-server UNIX socket at '{}'",
                self.socket_path.display()
            )
        })?;
        let mut connection = Connection::new(stream);
        debug!("Connected to socket");

        trace!(?request);
        connection.write_frame(&request)?;

        let response = connection.read_frame()?;

        Ok(response)
    }
}
