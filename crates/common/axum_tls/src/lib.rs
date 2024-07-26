//! A crate to handle creating secured [axum] servers with sensible, consistent defaults for thin-edge.
//!
//! # Introduction
//! Thin-edge has a number of requirements in order to maximise the value of HTTPS services
//! it executes, for instance:
//!
//! - Services should use **HTTP by default**, as this makes it easy to get thin-edge up and running
//! - Customers should have **the option to enable HTTPS**, which allows clients to trust they are connecting
//!   to the intended host, as well as encrypting any sensitive communication that may occur between
//!   the two
//! - Customers should have **the option to enable authentication**, which allows servers to decide whether a
//!   client is trusted before granting them access to potentially sensitive information
//!
//! Additionally, when enabling HTTPS, clients shouldn't need to establish whether the server
//! is HTTPS enabled or not (e.g. if I visit <http://google.com> in my browser, it redirects me to
//! <https://google.com>). Similarly, if I try to upload or download via HTTP when I have enabled
//! HTTPS for the file transfer service, the request should succeed (assuming the client in question
//! trusts the server certificate). This limits the pain associated with migrating from HTTP to
//! HTTPS, as the components connecting to thin-edge services don't need to know whether HTTPS is
//! enabled or not, just the server.
//!
//! # Authentication
//! Authentication for thin-edge HTTP services is handled using certificate-based authentication,
//! the same mechanism as what is used to authenticate an MQTT connection to Cumulocity.
#[cfg(doc)]
pub mod acceptor;
#[cfg(not(doc))]
mod acceptor;
pub mod config;
#[cfg(any(test, feature = "error-matching"))]
mod error_matching;
mod files;
mod maybe_tls;
#[cfg(doc)]
pub mod redirect_http;
#[cfg(not(doc))]
mod redirect_http;

use crate::acceptor::Acceptor;
pub use crate::acceptor::TlsData;
pub use crate::files::*;
use crate::redirect_http::redirect_http_to_https;
use axum::middleware::map_request;
use axum::Router;
#[cfg(feature = "error-matching")]
pub use error_matching::*;
use std::future::Future;
use std::net::TcpListener;

/// Starts a server with TLS
pub fn start_tls_server(
    listener: TcpListener,
    server_config: rustls::ServerConfig,
    app: Router,
) -> impl Future<Output = std::io::Result<()>> {
    axum_server::from_tcp(listener)
        .acceptor(Acceptor::new(server_config))
        .serve(
            app.layer(map_request(redirect_http_to_https))
                .into_make_service(),
        )
}
