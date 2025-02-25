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

// server.rs
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;

fn main() {
    // Define the path for the UNIX socket
    let socket_path = "/tmp/rust_unix_socket";

    // Remove the socket file if it already exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path).unwrap();
    }

    // Create a UNIX listener
    let listener = UnixListener::bind(socket_path).unwrap();
    println!("Server listening on {}", socket_path);

    // Accept a connection
    match listener.accept() {
        Ok((mut stream, _)) => {
            println!("Accepted a connection");

            // Read data from the client
            let mut buffer = [0; 1024];
            match stream.read(&mut buffer) {
                Ok(n) => {
                    println!("Received data: {}", String::from_utf8_lossy(&buffer[..n]));

                    // Send a response back to the client
                    stream.write_all(b"Hello from server").unwrap();
                }
                Err(e) => eprintln!("Failed to read from socket: {}", e),
            }
        }
        Err(e) => eprintln!("Failed to accept connection: {}", e),
    }
}
