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

use std::os::unix::net::UnixListener;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use clap::command;
use clap::Parser;
use tracing::debug;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing::warn;
use tracing_subscriber::EnvFilter;

use tedge_p11_server::CryptokiConfigDirect;
use tedge_p11_server::TedgeP11Server;

/// thin-edge.io service for passing PKCS#11 cryptographic tokens.
#[derive(Debug, Clone, PartialEq, Eq, Parser)]
#[command(version)]
pub struct Args {
    /// A path where the UNIX socket listener will be created.
    #[arg(long, default_value = "./tedge-p11-server.sock")]
    socket_path: Utf8PathBuf,

    /// The path to the PKCS#11 module.
    #[arg(long)]
    module_path: Utf8PathBuf,

    /// The PIN for the PKCS#11 token.
    #[arg(long, default_value = "123456")]
    pin: Arc<str>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();

    let args = Args::parse();
    let socket_path = args.socket_path;
    let cryptoki_config = CryptokiConfigDirect {
        module_path: args.module_path,
        pin: args.pin,
        serial: None,
    };

    info!(?cryptoki_config, "Using cryptoki configuration");

    let listener = {
        let mut systemd_listeners = sd_listen_fds::get()
            .context("Failed to obtain activated sockets from systemd")?
            .into_iter();
        if systemd_listeners.len() > 1 {
            warn!("Received multiple sockets but only first is used, rest are ignored");
        }
        if let Some((name, fd)) = systemd_listeners.next() {
            debug!(?name, "Using socket passed by systemd");
            UnixListener::from(fd)
        } else {
            debug!("No sockets from systemd, creating a standalone socket");
            UnixListener::bind(socket_path).context("Failed to bind to socket")?
        }
    };
    info!(listener = ?listener.local_addr().as_ref().ok().and_then(|s| s.as_pathname()), "Server listening");
    TedgeP11Server::from_config(cryptoki_config).serve(listener)?;

    Ok(())
}
