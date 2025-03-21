use crate::pkcs11;
use crate::pkcs11::CryptokiConfigDirect;

use crate::pkcs11::Pkcs11SigningKey;
use crate::pkcs11::PkcsSigner;

use rustls::sign::SigningKey;
use serde::Deserialize;
use serde::Serialize;
use tracing::instrument;

#[derive(Debug)]
pub struct P11SignerService {
    signing_key: Pkcs11SigningKey,
}

impl P11SignerService {
    // TODO(marcel): would be nice to check if there are any keys upon starting the server and warn the user if there is not
    pub fn new(config: &CryptokiConfigDirect) -> Self {
        let signing_key = pkcs11::Pkcs11SigningKey::from_cryptoki_config(config)
            .expect("failed to get pkcs11 signing key");

        Self { signing_key }
    }

    #[instrument]
    pub fn choose_scheme(&self, request: ChooseSchemeRequest) -> ChooseSchemeResponse {
        let offered = request.offered.into_iter().map(|s| s.0).collect::<Vec<_>>();

        let signer = self.signing_key.choose_scheme(&offered);
        let algorithm = SignatureAlgorithm(self.signing_key.algorithm());

        let Some(signer) = signer else {
            return ChooseSchemeResponse {
                scheme: None,
                algorithm,
            };
        };

        ChooseSchemeResponse {
            scheme: Some(SignatureScheme(signer.scheme())),
            algorithm,
        }
    }

    #[instrument]
    pub fn sign(&self, request: SignRequest) -> SignResponse {
        let session = match &self.signing_key {
            Pkcs11SigningKey::Ecdsa(key) => &key.pkcs11,
            Pkcs11SigningKey::Rsa(key) => &key.pkcs11,
        };
        let signer = PkcsSigner::from_session(session.clone());
        let signature = signer.sign(&request.to_sign).unwrap();
        SignResponse(signature)
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
