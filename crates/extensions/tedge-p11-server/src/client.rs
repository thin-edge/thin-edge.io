use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use tracing::debug;
use tracing::trace;

use super::connection::Frame1;
use super::service::ChooseSchemeRequest;
use super::service::SignRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11Client {
    pub socket_path: Arc<Path>,
}

impl TedgeP11Client {
    pub fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> anyhow::Result<Option<rustls::SignatureScheme>> {
        trace!("Connecting to socket...");
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);

        debug!("Connected to socket");

        let request = Frame1::ChooseSchemeRequest(ChooseSchemeRequest {
            offered: offered
                .iter()
                .copied()
                .map(super::service::SignatureScheme)
                .collect::<Vec<_>>(),
        });
        connection.write_frame(&request)?;

        let response = connection.read_frame()?;

        let Frame1::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme");
        };

        debug!("Choose scheme complete");

        let Some(scheme) = response.scheme else {
            return Ok(None);
        };

        Ok(Some(scheme.0))
    }

    // this function is called only on the server when handling ClientHello message, so
    // realistically it won't ever be called in our case
    pub fn algorithm(&self) -> anyhow::Result<rustls::SignatureAlgorithm> {
        trace!("Connecting to socket...");
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);

        debug!("Connected to socket");

        // if passed empty set of schemes, service doesn't return a scheme but returns an algorithm
        let request = Frame1::ChooseSchemeRequest(ChooseSchemeRequest { offered: vec![] });
        connection.write_frame(&request)?;

        let response = connection.read_frame()?;

        let Frame1::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme");
        };

        debug!("Choose scheme complete");

        Ok(response.algorithm.0)
    }

    pub fn sign(&self, message: &[u8]) -> anyhow::Result<Vec<u8>> {
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);
        debug!("Connected to socket");

        let request = Frame1::SignRequest(SignRequest {
            to_sign: message.to_vec(),
        });
        connection.write_frame(&request)?;

        let response = connection.read_frame()?;

        let Frame1::SignResponse(response) = response else {
            bail!("protocol error: bad response, expected sign");
        };

        debug!("Sign complete");

        Ok(response.0)
    }
}
