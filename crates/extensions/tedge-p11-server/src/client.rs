use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::Arc;

use anyhow::bail;
use tracing::debug;
use tracing::trace;

use crate::connection::Payload;

use super::connection::Frame;
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

        let request = Frame::new(Payload::ChooseSchemeRequest(ChooseSchemeRequest {
            offered: offered
                .iter()
                .copied()
                .map(super::service::SignatureScheme)
                .collect::<Vec<_>>(),
        }));
        connection.write_frame(&request)?;

        let response = connection.read_frame()?.payload;

        let Payload::ChooseSchemeResponse(response) = response else {
            bail!("protocol error: bad response, expected chose scheme");
        };

        debug!("Choose scheme complete");

        let Some(scheme) = response.scheme else {
            return Ok(None);
        };

        Ok(Some(scheme.0))
    }

    pub fn sign(&self, message: &[u8]) -> anyhow::Result<Vec<u8>> {
        let stream = UnixStream::connect(&self.socket_path)?;
        let mut connection = crate::connection::Connection::new(stream);
        debug!("Connected to socket");

        let request = Frame::new(Payload::SignRequest(SignRequest {
            to_sign: message.to_vec(),
        }));
        connection.write_frame(&request)?;

        let response = connection.read_frame()?.payload;

        let Payload::SignResponse(response) = response else {
            bail!("protocol error: bad response, expected sign");
        };

        debug!("Sign complete");

        Ok(response.0)
    }
}
