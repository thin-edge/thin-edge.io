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

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::pkcs11::PkcsSigner;
use certificate::parse_root_certificate::{pkcs11, CryptokiConfigDirect};
use clap::command;
use clap::Parser;
use tracing::debug;
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
    ///
    /// If not provided, the module path will be read from tedge-config.
    #[arg(long)]
    module_path: Option<Utf8PathBuf>,

    /// The PIN for the PKCS#11 token.
    ///
    /// If not provided, the PIN will be read from tedge-config.
    #[arg(long, default_value = "123456")]
    pin: Arc<str>,
}

fn main() -> anyhow::Result<()> {
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

    let cryptoki_config = if let Some(module_path) = args.module_path {
        CryptokiConfigDirect {
            module_path,
            pin: args.pin,
            serial: None,
        }
    } else if let Some(cryptoki_config) = tedge_config::TEdgeConfigLocation::default()
        .load()
        .ok()
        .and_then(|tedge_config| tedge_config.device.cryptoki.config_direct().ok().flatten())
    {
        debug!("Using cryptoki config from tedge-config");
        cryptoki_config
    } else {
        return Err(anyhow::anyhow!(
            "Need to provide module_path via argument or tedge-config"
        ));
    };

    info!(?cryptoki_config, "Using cryptoki configuration");

    let signing_key = pkcs11::Pkcs11SigningKey::from_cryptoki_config(cryptoki_config)
        .expect("failed to get pkcs11 signing key");

    let session = match signing_key {
        pkcs11::Pkcs11SigningKey::Ecdsa(e) => e.pkcs11,
        _ => panic!("Expected a session"),
    };
    let signer = PkcsSigner::from_session(session);

    let listener = UnixListener::bind(&socket_path).context("Failed to bind to socket")?;
    info!(%socket_path, "Server listening");

    // Accept a connection
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                info!("Accepted a connection");

                // Read data from the client
                let mut buffer = [0; 1024];
                match stream.read(&mut buffer) {
                    Ok(n) => {
                        let message = &buffer[..n];
                        debug!(message = ?message, "Received data");

                        process_message(&mut stream, message, &signer);
                    }
                    Err(e) => eprintln!("Failed to read from socket: {}", e),
                }
            }
            Err(e) => eprintln!("Failed to accept connection: {}", e),
        }
    }
}

fn choose_scheme(offered: &str) -> &str {
    "ECDSA"
}

fn process_message(stream: &mut UnixStream, message: &[u8], signing_key: &PkcsSigner) {
    let mut buffer = BufReader::new(message);
    let mut line = String::new();
    buffer.read_line(&mut line).unwrap();
    debug!(%line);

    if line.starts_with("offered =") {
        handle_choose_scheme(stream);
    } else if line.starts_with("sign:") {
        handle_sign_request(stream, &mut buffer, signing_key);
    }
}

fn handle_choose_scheme(stream: &mut UnixStream) {
    let scheme = choose_scheme("ECDSA");
    writeln!(stream, "{}", scheme).unwrap();
}

fn handle_sign_request(
    stream: &mut UnixStream,
    buffer: &mut BufReader<&[u8]>,
    signing_key: &PkcsSigner,
) {
    let mut to_sign = buffer.fill_buf().unwrap();

    let mut buf = [0u8; 1024];
    if to_sign.is_empty() {
        let n = stream.read(&mut buf).unwrap();
        to_sign = &buf[..n];
    }

    debug!(?to_sign);
    let signature = signing_key.sign(to_sign).unwrap();
    stream.write_all(&signature).unwrap();
    info!(len = signature.len(), "written signature");
}
