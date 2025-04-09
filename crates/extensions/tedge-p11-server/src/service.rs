use crate::pkcs11::Cryptoki;
use crate::pkcs11::CryptokiConfigDirect;

use crate::pkcs11::Pkcs11SigningKey;
use crate::pkcs11::PkcsSigner;

use anyhow::Context;
use rustls::sign::SigningKey;
use serde::Deserialize;
use serde::Serialize;
use tracing::instrument;

pub trait SigningService {
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse>;
    fn sign(&self, request: SignRequest) -> anyhow::Result<SignResponse>;
}

#[derive(Debug)]
pub struct TedgeP11Service {
    cryptoki: Cryptoki,
}

impl TedgeP11Service {
    // TODO(marcel): would be nice to check if there are any keys upon starting the server and warn the user if there is not
    pub fn new(config: CryptokiConfigDirect) -> anyhow::Result<Self> {
        let cryptoki = Cryptoki::new(config).context("Failed to load cryptoki library")?;

        Ok(Self { cryptoki })
    }
}

impl SigningService for TedgeP11Service {
    #[instrument(skip_all)]
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse> {
        let offered = request.offered.into_iter().map(|s| s.0).collect::<Vec<_>>();

        let signing_key = self
            .cryptoki
            .signing_key()
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
    fn sign(&self, request: SignRequest) -> anyhow::Result<SignResponse> {
        let signing_key = self
            .cryptoki
            .signing_key()
            .context("Failed to find a signing key")?;

        let session = match signing_key {
            Pkcs11SigningKey::Ecdsa(key) => key.pkcs11,
            Pkcs11SigningKey::Rsa(key) => key.pkcs11,
        };
        let signer = PkcsSigner::from_session(session.clone());
        let signature = signer
            .sign(&request.to_sign)
            .context("Failed to sign using PKCS #11")?;
        Ok(SignResponse(signature))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseSchemeRequest {
    pub offered: Vec<SignatureScheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseSchemeResponse {
    pub scheme: Option<SignatureScheme>,
    pub algorithm: SignatureAlgorithm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignRequest {
    pub to_sign: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignResponse(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
