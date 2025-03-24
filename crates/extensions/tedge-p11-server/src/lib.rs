/// The signer service that handles requests.
mod service;

/// A server listening on the UNIX domain socket, wrapping the service.
mod server;
pub use server::TedgeP11Server;

/// Serialization and framing of messages sent between the client and server.
mod connection;

/// A rustls SigningKey that connects to the server.
mod signer;
pub use signer::signing_key;
pub use signer::CryptokiConfig;

/// A client that connects to the UNIX server, used by the signer.
pub mod client;

/// Interfaces with the PKCS#11 dynamic module using cryptoki crate.
mod pkcs11;
pub use pkcs11::CryptokiConfigDirect;

pub mod single_cert_and_key;
