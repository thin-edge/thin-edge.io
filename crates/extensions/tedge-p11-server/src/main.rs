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
use cryptoki::types::AuthPin;
use tedge_p11_server::service::TedgeP11Service;
use tokio::signal::unix::SignalKind;
use tracing::debug;
use tracing::info;
use tracing::warn;
use tracing::Level;
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
    pin: String,

    /// A URI of the token/object to use.
    ///
    /// See RFC #7512.
    #[arg(long)]
    uri: Option<Arc<str>>,

    /// Configures the logging level.
    ///
    /// One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level
    /// will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
    #[arg(long)]
    log_level: Option<Level>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_file(true)
        .with_line_number(true)
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(args.log_level.unwrap_or(Level::INFO).into())
                .from_env()
                .unwrap(),
        )
        .init();

    let socket_path = args.socket_path;
    let cryptoki_config = CryptokiConfigDirect {
        module_path: args.module_path,
        pin: AuthPin::new(args.pin),
        serial: None,
        uri: args.uri.filter(|s| !s.is_empty()),
    };

    info!(?cryptoki_config, "Using cryptoki configuration");

    // make sure that if we bind to unix socket in the program, it's removed on exit
    let (listener, _drop_guard) = {
        let mut systemd_listeners = sd_listen_fds::get()
            .context("Failed to obtain activated sockets from systemd")?
            .into_iter();
        if systemd_listeners.len() > 1 {
            warn!("Received multiple sockets but only first is used, rest are ignored");
        }
        if let Some((name, fd)) = systemd_listeners.next() {
            debug!(?name, "Using socket passed by systemd");
            let listener = UnixListener::from(fd);
            (listener, None)
        } else {
            debug!("No sockets from systemd, creating a standalone socket");
            let socket_dir = socket_path.parent();
            if let Some(socket_dir) = socket_dir {
                if !socket_dir.exists() {
                    tokio::fs::create_dir_all(socket_dir)
                        .await
                        .context(format!(
                            "error creating parent directories for {socket_dir:?}"
                        ))?;
                }
            }
            let listener = UnixListener::bind(&socket_path)
                .with_context(|| format!("Failed to bind to socket at '{socket_path}'"))?;
            (
                listener,
                Some(OwnedSocketFileDropGuard(socket_path.clone())),
            )
        }
    };
    info!(listener = ?listener.local_addr().as_ref().ok().and_then(|s| s.as_pathname()), "Server listening");
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    let service =
        TedgeP11Service::new(cryptoki_config).context("Failed to create the signing service")?;
    let server = TedgeP11Server::new(service)?;
    tokio::spawn(async move { server.serve(listener).await });

    // by capturing SIGINT and SIGERM, we allow owned socket drop guard to run before exit
    let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = sigint.recv() => {}
        _ = sigterm.recv() => {}
    }

    Ok(())
}

struct OwnedSocketFileDropGuard(Utf8PathBuf);

// necessary for correct unix socket deletion when server exits with an error
impl Drop for OwnedSocketFileDropGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
