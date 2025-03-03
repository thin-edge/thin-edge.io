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
use std::path::{Path, PathBuf};
use std::sync::Arc;

use camino::Utf8PathBuf;
use certificate::parse_root_certificate::pkcs11::{self, PkcsSigner};
use tracing::{debug, info, trace};

fn main() {
    tracing_subscriber::fmt::init();

    let socket_path = "/tmp/rust_unix_socket";

    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path).unwrap();
    }

    let listener = UnixListener::bind(socket_path).unwrap();
    println!("Server listening on {}", socket_path);

    let config = certificate::parse_root_certificate::CryptokiConfig {
        module_path: Utf8PathBuf::from("/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so"),
        pin: Arc::from("123456"),
        serial: None,
    };
    let signing_key = pkcs11::Pkcs11SigningKey::from_cryptoki_config(config)
        .expect("failed to get pkcs11 signing key");

    let session = match signing_key {
        pkcs11::Pkcs11SigningKey::Ecdsa(e) => e.pkcs11,
        _ => panic!("Expected a session"),
    };
    let signer = PkcsSigner::from_session(session);

    // Accept a connection
    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                println!("Accepted a connection");

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
