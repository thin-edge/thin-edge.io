use crate::pkcs11::Cryptoki;
use crate::pkcs11::CryptokiConfigDirect;
use crate::pkcs11::SigScheme;

use anyhow::Context;
use rustls::sign::SigningKey;
use serde::Deserialize;
use serde::Serialize;
use tracing::instrument;
use tracing::trace;
use tracing::warn;

/// The main PKCS #11 trait, allowing callers to perform operations on the PKCS #11 token.
pub trait TedgeP11Service: Send + Sync {
    /// Given a set of proposed signature schemes, returns a signature scheme that can be used by the private key object
    /// on the token (denoted by uri) for signing.
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse>;

    /// Signs the message using the private key object on the token (denoted by uri).
    fn sign(&self, request: SignRequestWithSigScheme) -> anyhow::Result<SignResponse>;
}

#[derive(Debug)]
pub struct TedgeP11 {
    cryptoki: Cryptoki,
}

impl TedgeP11 {
    // TODO(marcel): would be nice to check if there are any keys upon starting the server and warn the user if there is not
    pub fn new(config: CryptokiConfigDirect) -> anyhow::Result<Self> {
        let cryptoki = Cryptoki::new(config).context("Failed to load cryptoki library")?;

        // try to find a key on startup to see if requests succeed if nothing changes
        if cryptoki.signing_key(None).is_err() {
            warn!("No signing key found");
        }

        Ok(Self { cryptoki })
    }
}

impl TedgeP11Service for TedgeP11 {
    #[instrument(skip_all)]
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse> {
        trace!(?request);
        let offered = request.offered.into_iter().map(|s| s.0).collect::<Vec<_>>();
        let uri = request.uri;

        let signing_key = self
            .cryptoki
            .signing_key(uri.as_deref())
            .context("Failed to find a signing key")?;

        let signer = signing_key.choose_scheme(&offered);
        let algorithm = SignatureAlgorithm(signing_key.algorithm());

        let Some(signer) = signer else {
            return Ok(ChooseSchemeResponse {
                scheme: None,
                algorithm,
            });
        };

        Ok(ChooseSchemeResponse {
            scheme: Some(SignatureScheme(signer.scheme())),
            algorithm,
        })
    }

    #[instrument(skip_all)]
    fn sign(&self, request: SignRequestWithSigScheme) -> anyhow::Result<SignResponse> {
        trace!(?request);
        let uri = request.uri;
        let signer = self
            .cryptoki
            .signing_key(uri.as_deref())
            .context("Failed to find a signing key")?;

        let signature = signer
            .sign(&request.to_sign, request.sigscheme)
            .context("Failed to sign using PKCS #11")?;
        Ok(SignResponse(signature))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseSchemeRequest {
    pub offered: Vec<SignatureScheme>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseSchemeResponse {
    pub scheme: Option<SignatureScheme>,
    pub algorithm: SignatureAlgorithm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignRequest {
    pub to_sign: Vec<u8>,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignRequestWithSigScheme {
    pub to_sign: Vec<u8>,
    pub uri: Option<String>,
    pub sigscheme: Option<SigScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignResponse(pub Vec<u8>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureScheme(pub rustls::SignatureScheme);

impl Serialize for SignatureScheme {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        u16::from(self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignatureScheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u16::deserialize(deserializer)?;
        Ok(Self(rustls::SignatureScheme::from(value)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureAlgorithm(pub rustls::SignatureAlgorithm);

impl Serialize for SignatureAlgorithm {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        u8::from(self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignatureAlgorithm {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Ok(Self(rustls::SignatureAlgorithm::from(value)))
    }
}
