//! thin-edge.io PKCS#11 server.
//!
//! The purpose of this crate is to allow thin-edge services that possibly run in containers to access PKCS#11 tokens in
//! all of our supported architectures.
//!
//! There are 2 main problems with using a PKCS#11 module directly by thin-edge:
//! 1. One needs to use a dynamic loader to load the PKCS#11 module, which is not possible in statically compiled musl
//! 2. When thin-edge runs in a container, additional setup needs to be done by the user to expose cryptographic tokens
//!    in the container, using software like p11-kit.
//!
//! To avoid extra dependencies and possibly implement new features in the future, it was decided that thin-edge.io will
//! provide its own bundled p11-kit-like service.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use clap::command;
use clap::Parser;
use tedge_p11_server::pkcs11::CryptokiConfigDirect;
use tedge_p11_server::P11Server;
use tedge_p11_server::P11Service;
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

/// thin-edge.io service for passing PKCS#11 cryptographic tokens.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(version)]
pub struct Args {
    /// A path where the service's unix token will be created.
    #[arg(default_value = "./thin-edge-pkcs11.sock")]
    socket_path: Utf8PathBuf,

    /// The path to the PKCS#11 module.
    #[arg(long)]
    module_path: Utf8PathBuf,

    /// The PIN for the PKCS#11 token.
    #[arg(long, default_value = "123456")]
    pin: Arc<str>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();

    let args = Args::parse();
    let socket_path = args.socket_path;

    if Path::new(&socket_path).exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let cryptoki_config = CryptokiConfigDirect {
        module_path: args.module_path,
        pin: args.pin,
        serial: None,
    };

    info!(?cryptoki_config, "Using cryptoki configuration");

    let incoming = UnixListenerStream::new(
        UnixListener::bind(&socket_path).context("Failed to bind to socket")?,
    );

    let service =
        P11Service::from_config(cryptoki_config).context("Failed to create P11Service")?;

    info!(%socket_path, "Server listening");

    Server::builder()
        .add_service(P11Server::new(service))
        .serve_with_incoming(incoming)
        .await?;

    Ok(())
}
