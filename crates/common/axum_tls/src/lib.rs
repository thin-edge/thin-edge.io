mod acceptor;
mod files;
mod maybe_tls;
mod redirect_http;

use crate::acceptor::Acceptor;
pub use crate::files::*;
use crate::redirect_http::redirect_http_to_https;
use axum::middleware::map_request;
use axum::Router;
use std::future::Future;
use std::net::TcpListener;

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
