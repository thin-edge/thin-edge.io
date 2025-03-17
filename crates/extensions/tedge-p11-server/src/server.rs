use std::os::unix::net::UnixListener;

use anyhow::Context;
use camino::Utf8Path;
use tracing::debug;
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

    pub fn serve(&self, socket_path: &Utf8Path) -> anyhow::Result<()> {
        let listener = UnixListener::bind(socket_path).context("Failed to bind to socket")?;

        // Accept a connection
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    debug!("Accepted a connection");

                    let connection = Connection::new(stream);

                    match process(&self.config, connection) {
                        Ok(_) => info!("Incoming request successful"),
                        Err(e) => error!("Incoming request failed: {e}"),
                    }
                }
                Err(e) => error!("Failed to accept connection: {}", e),
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
