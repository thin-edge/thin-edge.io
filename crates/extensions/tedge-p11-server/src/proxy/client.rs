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
use crate::proxy::frame::Frame;
use crate::proxy::frame::Frame2;
use crate::proxy::frame1::VersionInfo;
use crate::service::ChooseSchemeRequest;
use crate::service::ChooseSchemeResponse;
use crate::service::CreateKeyRequest;
use crate::service::SecretString;
use crate::service::SignRequest;
use crate::service::SignRequestWithSigScheme;
use crate::service::TedgeP11Service;

/// A [`TedgeP11Service`] implementation that proxies requests to the [`TedgeP11Server`](super::TedgeP11Server) that
/// does the operations.
#[derive(Debug, Clone)]
pub struct TedgeP11Client {
    pub(crate) socket_path: Arc<Path>,
    pub(crate) uri: Option<Arc<str>>,
    pub(crate) pin: Option<SecretString>,
    server_version: Option<VersionInfo>,
}

impl TedgeP11Service for TedgeP11Client {
    fn choose_scheme(
        &self,
        request: ChooseSchemeRequest,
    ) -> anyhow::Result<crate::service::ChooseSchemeResponse> {
        let uri = request
            .uri
            .as_deref()
            .or(self.uri.as_deref())
            .map(ToString::to_string);
        let offered: Vec<_> = request.offered.iter().map(|s| s.0).collect();
        self.choose_scheme(&offered, uri)
    }

    fn sign(
        &self,
        request: SignRequestWithSigScheme,
    ) -> anyhow::Result<crate::service::SignResponse> {
        let uri = request
            .uri
            .as_deref()
            .or(self.uri.as_deref())
            .map(ToString::to_string);
        let response = match request.sigscheme {
            Some(sigscheme) => self.sign2(&request.to_sign, uri, sigscheme)?,
            None => self.sign(&request.to_sign, uri)?,
        };

        Ok(crate::service::SignResponse(response))
    }

    fn get_public_key_pem(&self, uri: Option<&str>) -> anyhow::Result<String> {
        let uri = uri.or(self.uri.as_deref()).map(ToString::to_string);
        self.get_public_key_pem(uri)
    }

    fn create_key(
        &self,
        _request: CreateKeyRequest,
    ) -> anyhow::Result<crate::service::CreateKeyResponse> {
        self.create_key(_request)
    }

    fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
        let request = Frame1::GetTokensUrisRequest;
        let response = self.do_request(request)?;

        let Frame1::GetTokensUrisResponse(uris) = response else {
            bail!("protocol error: bad response, expected get_uris, received: {response:?}");
        };

        Ok(uris)
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
        let mut client = Self {
            socket_path,
            uri: None,
            pin: None,
            server_version: None,
        };

        // make any request to make sure the service is online and it will respond
        let version_info = client.ping().unwrap_or(None);
        client.server_version = version_info;

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
            pin: self.pin.clone(),
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
            pin: self.pin.clone(),
        });
        let response = self.do_request(request)?;

        let Frame1::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme, received: {response:?}");
        };

        debug!("Choose scheme complete");

        Ok(response.algorithm.0)
    }

    pub fn sign(&self, message: &[u8], uri: Option<String>) -> anyhow::Result<Vec<u8>> {
        let request = Frame2::SignRequest(SignRequest {
            to_sign: message.to_vec(),
            uri,
            pin: self.pin.clone(),
        });
        let response = self.do_request2(request)?;

        let Frame2::SignResponse(response) = response else {
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
            pin: self.pin.clone(),
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
            bail!(
                "protocol error: bad response, expected get_public_key_pem, received: {response:?}"
            );
        };

        Ok(pubkey_pem)
    }

    pub fn ping(&self) -> anyhow::Result<Option<VersionInfo>> {
        let request = Frame1::Ping;
        let response = self.do_request(request)?;

        let Frame1::Pong(version_info) = response else {
            bail!("protocol error: bad response, expected pong, received: {response:?}");
        };

        Ok(version_info)
    }

    pub fn create_key(
        &self,
        request: CreateKeyRequest,
    ) -> anyhow::Result<crate::service::CreateKeyResponse> {
        let request = Frame1::CreateKeyRequest(request);
        let response = self.do_request(request)?;

        let Frame1::CreateKeyResponse(pubkey) = response else {
            bail!("protocol error: bad response, expected create_key, received: {response:?}");
        };

        Ok(pubkey)
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
        connection.write_frame(&Frame::Version1(request))?;

        let Frame::Version1(response) = connection.read_frame()? else {
            bail!("protocol error: bad response, expected version 1 frame");
        };

        Ok(response)
    }

    fn do_request2(&self, request: Frame2) -> anyhow::Result<Frame2> {
        let stream = UnixStream::connect(&self.socket_path).with_context(|| {
            format!(
                "Failed to connect to tedge-p11-server UNIX socket at '{}'",
                self.socket_path.display()
            )
        })?;
        let mut connection = Connection::new(stream);
        debug!("Connected to socket");

        trace!(?request);
        connection.write_frame2(&request)?;

        let Frame::Version2(response) = connection.read_frame()? else {
            bail!("protocol error: bad response, expected version 2 frame");
        };

        Ok(response)
    }
}
