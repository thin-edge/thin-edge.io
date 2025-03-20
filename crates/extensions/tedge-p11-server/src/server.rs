use std::os::unix::net::UnixListener;

use anyhow::Context;
use tracing::error;
use tracing::info;

use super::connection::Connection;
use super::connection::Payload;
use crate::connection::Frame;
use crate::pkcs11::CryptokiConfigDirect;
use crate::service::P11SignerService;

pub struct TedgeP11Server {
    config: CryptokiConfigDirect,
}

impl TedgeP11Server {
    pub fn from_config(config: CryptokiConfigDirect) -> Self {
        Self { config }
    }

    /// Handle multiple requests on a given listener.
    pub fn serve(&self, listener: UnixListener) -> anyhow::Result<()> {
        // Accept a connection
        loop {
            let (stream, _) = listener.accept().context("Failed to accept connection")?;

            let connection = Connection::new(stream);

            match process(&self.config, connection) {
                Ok(_) => info!("Incoming request successful"),
                Err(e) => error!("Incoming request failed: {e:?}"),
            }
        }
    }
}

fn process(config: &CryptokiConfigDirect, mut connection: Connection) -> anyhow::Result<()> {
    let service = P11SignerService::new(config);

    let request = connection.read_frame()?.payload;

    let response = match request {
        Payload::ChooseSchemeResponse { .. } | Payload::SignResponse { .. } => {
            anyhow::bail!("protocol error")
        }
        Payload::ChooseSchemeRequest(request) => {
            Payload::ChooseSchemeResponse(service.choose_scheme(request))
        }
        Payload::SignRequest(request) => Payload::SignResponse(service.sign(request)),
    };
    let response = Frame::new(response);

    connection.write_frame(&response)?;

    Ok(())
}
