use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;

pub mod service;

/// Returns a `TedgeP11Service` implementation that depending on the config, either connects to tedge-p11-server or
/// calls cryptoki module directly.
pub fn tedge_p11_service(config: CryptokiConfig) -> anyhow::Result<Arc<dyn TedgeP11Service>> {
    let signing_key: Arc<dyn TedgeP11Service> = match config {
        CryptokiConfig::Direct(config_direct) => {
            let cryptoki =
                pkcs11::Cryptoki::new(config_direct).context("Failed to load cryptoki library")?;
            Arc::new(cryptoki)
        }
        CryptokiConfig::SocketService { socket_path, uri } => {
            let mut client = client::TedgeP11Client::with_ready_check(socket_path.into());
            client.uri = uri;
            Arc::new(client)
        }
    };
    Ok(signing_key)
}

/// A server listening on the UNIX domain socket, wrapping the service.
mod server;
pub use server::TedgeP11Server;

/// Serialization and framing of messages sent between the client and server.
mod connection;

/// A rustls SigningKey that connects to the server.
mod signer;
pub use signer::signing_key;

/// A client that connects to the UNIX server, used by the signer.
pub mod client;

/// Interfaces with the PKCS#11 dynamic module using cryptoki crate.
pub mod pkcs11;
pub use pkcs11::AuthPin;
pub use pkcs11::CryptokiConfigDirect;

use crate::service::TedgeP11Service;

pub mod single_cert_and_key;

#[derive(Debug, Clone)]
pub enum CryptokiConfig {
    Direct(CryptokiConfigDirect),
    SocketService {
        socket_path: Utf8PathBuf,
        uri: Option<Arc<str>>,
    },
}
