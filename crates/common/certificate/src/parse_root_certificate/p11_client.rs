//! A rustls signing key that uses tedge-p11-server for signing.

use std::io::{Read, Write};
use std::{os::unix::net::UnixStream, path::Path, sync::Arc};

use rustls::sign::{Signer, SigningKey};
use rustls::SignatureAlgorithm;
use tracing::{info, instrument};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11Client {
    pub socket_path: Arc<Path>,
}

impl SigningKey for TedgeP11Client {
    #[instrument]
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        let mut stream = UnixStream::connect(&self.socket_path).unwrap();
        writeln!(&mut stream, "offered = {offered:?}").unwrap();

        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).unwrap();

        if buffer[..n].starts_with(b"ECDSA") {
            return Some(Box::new(TedgeP11ClientSigner {
                socket_path: self.socket_path.clone(),
            }));
        }

        None
    }

    #[instrument]
    fn algorithm(&self) -> SignatureAlgorithm {
        let mut stream = UnixStream::connect(&self.socket_path).unwrap();
        writeln!(&mut stream, "algorithm").unwrap();
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).unwrap();
        info!(
            response = %String::from_utf8_lossy(&buffer[..n]),
            "Received data"
        );
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TedgeP11ClientSigner {
    pub socket_path: Arc<Path>,
}

impl Signer for TedgeP11ClientSigner {
    #[instrument]
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let mut stream = UnixStream::connect(&self.socket_path).unwrap();
        writeln!(&mut stream, "sign:").unwrap();
        info!(len = message.len(), "written message");
        stream.write_all(message).unwrap();

        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).unwrap();
        info!(response = ?buffer[..n], len = n, "Received data");

        Ok(buffer[..n].to_vec())
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        rustls::SignatureScheme::ECDSA_NISTP256_SHA256
    }
}
