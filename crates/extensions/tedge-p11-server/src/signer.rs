use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use tracing::instrument;

use crate::client::TedgeP11Client;
use crate::pkcs11::CryptokiConfigDirect;
use crate::pkcs11::Pkcs11SigningKey;

#[derive(Debug, Clone)]
pub enum CryptokiConfig {
    Direct(CryptokiConfigDirect),
    SocketService { socket_path: Utf8PathBuf },
}

/// Returns a rustls SigningKey that depending on the config, either connects to
/// tedge-p11-server or calls cryptoki module directly.
pub fn signing_key(config: CryptokiConfig) -> anyhow::Result<Arc<dyn SigningKey>> {
    let signing_key: Arc<dyn SigningKey> = match config {
        CryptokiConfig::Direct(config_direct) => Arc::new(
            Pkcs11SigningKey::from_cryptoki_config(&config_direct)
                .context("failed to create a TLS signer using PKCS#11 device")?,
        ),
        CryptokiConfig::SocketService { socket_path } => Arc::new(TedgeP11ClientSigningKey {
            socket_path: Arc::from(Path::new(&socket_path)),
        }),
    };

    Ok(signing_key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigningKey {
    pub socket_path: Arc<Path>,
}

impl SigningKey for TedgeP11ClientSigningKey {
    #[instrument(skip_all)]
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        let client = TedgeP11Client {
            socket_path: self.socket_path.clone(),
        };
        let response = client.choose_scheme(offered).unwrap();
        let scheme = response?;

        Some(Box::new(TedgeP11ClientSigner {
            socket_path: self.socket_path.clone(),
            scheme,
        }))
    }

    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        let client = TedgeP11Client {
            socket_path: self.socket_path.clone(),
        };

        client.algorithm().unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TedgeP11ClientSigner {
    pub socket_path: Arc<Path>,
    scheme: rustls::SignatureScheme,
}

impl Signer for TedgeP11ClientSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let client = TedgeP11Client {
            socket_path: self.socket_path.clone(),
        };
        let response = client.sign(message).unwrap();
        Ok(response)
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        self.scheme
    }
}
