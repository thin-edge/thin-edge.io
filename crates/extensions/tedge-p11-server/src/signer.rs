use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use camino::Utf8PathBuf;
use rustls::sign::SigningKey;

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
        CryptokiConfig::SocketService { socket_path } => {
            Arc::new(crate::client::TedgeP11ClientSigningKey {
                socket_path: Arc::from(Path::new(&socket_path)),
            })
        }
    };

    Ok(signing_key)
}
