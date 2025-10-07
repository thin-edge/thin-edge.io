use std::sync::Arc;

use anyhow::Context;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use secrecy::ExposeSecret;
use tracing::error;
use tracing::instrument;

use crate::pkcs11::Cryptoki;
use crate::pkcs11::Pkcs11Signer;
use crate::pkcs11::SessionParams;
use crate::pkcs11::SigScheme;
use crate::proxy::client::TedgeP11Client;
use crate::CryptokiConfig;

/// A signer using a private key object located on the PKCS11 token.
///
/// Is backed by either direct cryptoki library usage or by tedge-p11-server client.
///
/// Contains a handle to Pkcs11-backed private key that will be used for signing, selected at construction time.
pub trait TedgeP11Signer: SigningKey {
    /// Signs the message using the selected private key.
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>>;

    /// Signs the message using the selected private key and signature scheme.
    ///
    /// Useful when a key can be used with multiple schemes, eg. RSA key using PKCS 1.5 or PSS.
    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>>;

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey>;
}

impl TedgeP11Signer for Pkcs11Signer {
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        Pkcs11Signer::sign(self, msg, None)
    }

    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>> {
        Pkcs11Signer::sign(self, msg, Some(sigscheme))
    }

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey> {
        self
    }
}

/// Returns a rustls SigningKey that depending on the config, either connects to
/// tedge-p11-server or calls cryptoki module directly.
pub fn signing_key(config: CryptokiConfig) -> anyhow::Result<Arc<dyn TedgeP11Signer>> {
    let signing_key: Arc<dyn TedgeP11Signer> = match config {
        CryptokiConfig::Direct(config_direct) => {
            let uri = config_direct.uri.as_ref().map(|u| u.to_string());
            let pin = Some(crate::service::SecretString::new(
                config_direct.pin.expose_secret().clone(),
            ));
            let cryptoki =
                Cryptoki::new(config_direct).context("Failed to load cryptoki library")?;
            Arc::new(
                cryptoki
                    .signing_key_retry(SessionParams { uri, pin })
                    .context("failed to create a TLS signer using PKCS#11 device")?,
            )
        }
        CryptokiConfig::SocketService {
            socket_path,
            uri,
            pin,
        } => {
            let mut client = TedgeP11Client::with_ready_check(socket_path.into());
            client.pin = pin;
            Arc::new(TedgeP11ClientSigningKey { client, uri })
        }
    };

    Ok(signing_key)
}

#[derive(Debug, Clone)]
pub struct TedgeP11ClientSigningKey {
    pub client: TedgeP11Client,
    pub uri: Option<Arc<str>>,
}

impl TedgeP11Signer for TedgeP11ClientSigningKey {
    fn sign(&self, msg: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.client
            .sign(msg, self.uri.as_ref().map(|s| s.to_string()))
    }

    fn sign2(&self, msg: &[u8], sigscheme: SigScheme) -> anyhow::Result<Vec<u8>> {
        self.client
            .sign2(msg, self.uri.as_ref().map(|s| s.to_string()), sigscheme)
    }

    fn to_rustls_signing_key(self: Arc<Self>) -> Arc<dyn rustls::sign::SigningKey> {
        self
    }
}

impl SigningKey for TedgeP11ClientSigningKey {
    #[instrument(skip_all)]
    fn choose_scheme(
        &self,
        offered: &[rustls::SignatureScheme],
    ) -> Option<Box<dyn rustls::sign::Signer>> {
        let uri = self.uri.as_ref().map(|s| s.to_string());
        let response = match self.client.choose_scheme(offered, uri) {
            Ok(response) => response,
            Err(err) => {
                error!(?err, "Failed to choose scheme using cryptoki signer");
                return None;
            }
        };
        let scheme = response.scheme?.0;

        Some(Box::new(TedgeP11ClientSigner {
            client: self.client.clone(),
            scheme,
            uri: self.uri.clone(),
        }))
    }

    fn algorithm(&self) -> rustls::SignatureAlgorithm {
        // here we have no choice but to panic but this is only called by servers when verifying
        // client hello so it should never be called in our case
        self.client.algorithm().unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct TedgeP11ClientSigner {
    pub client: TedgeP11Client,
    scheme: rustls::SignatureScheme,
    pub uri: Option<Arc<str>>,
}

impl Signer for TedgeP11ClientSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let response = match self
            .client
            .sign(message, self.uri.as_ref().map(|s| s.to_string()))
        {
            Ok(response) => response,
            Err(err) => {
                return Err(rustls::Error::Other(rustls::OtherError(Arc::from(
                    Box::from(err),
                ))));
            }
        };
        Ok(response)
    }

    fn scheme(&self) -> rustls::SignatureScheme {
        self.scheme
    }
}
