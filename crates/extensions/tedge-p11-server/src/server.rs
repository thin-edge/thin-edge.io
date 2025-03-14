use std::os::unix::net::UnixListener;

use anyhow::Context;
use camino::Utf8Path;
use tracing::error;
use tracing::info;

use super::connection::Connection;
use crate::connection::Frame1;
use crate::connection::ProtocolError;
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

    let request = connection.read_frame()?;

    let response = match request {
        Frame1::Error(_) | Frame1::ChooseSchemeResponse { .. } | Frame1::SignResponse { .. } => {
            let error = ProtocolError("invalid request".to_string());
            let _ = connection.write_frame(&Frame1::Error(error));
            anyhow::bail!("protocol error: invalid request")
        }
        Frame1::ChooseSchemeRequest(request) => {
            Frame1::ChooseSchemeResponse(service.choose_scheme(request))
        }
        Frame1::SignRequest(request) => Frame1::SignResponse(service.sign(request)),
    };

    connection.write_frame(&response)?;

    Ok(())
}
