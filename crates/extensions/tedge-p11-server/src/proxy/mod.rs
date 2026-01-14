/// A server listening on the UNIX domain socket, wrapping the service.
pub mod server;
pub use server::TedgeP11Server;

/// A client that connects to the UNIX server, used by the signer.
pub mod client;
pub use client::TedgeP11Client;

/// Serialization and framing of messages sent between the client and server.
mod connection;

mod frame;
mod frame1;
mod frame2;

mod request;
mod response;

mod error;
