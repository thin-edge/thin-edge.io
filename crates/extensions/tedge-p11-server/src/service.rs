use crate::pkcs11::CreateKeyParams;
use crate::pkcs11::SigScheme;
use cryptoki::types::AuthPin;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Debug;
use std::fmt::Display;

/// The main PKCS #11 trait, allowing callers to perform operations on the PKCS #11 token.
pub trait TedgeP11Service: Send + Sync {
    /// Given a set of proposed signature schemes, returns a signature scheme that can be used by the private key object
    /// on the token (denoted by uri) for signing.
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse>;

    /// Signs the message using the private key object on the token (denoted by uri).
    fn sign(&self, request: SignRequestWithSigScheme) -> anyhow::Result<SignResponse>;

    /// Returns the public key in PEM format.
    ///
    /// Function will return public key PEM if `uri` identifies either a public key, or a private key with a matching
    /// key being present on the token.
    ///
    /// Note: in some cases PKCS 11 RSA private key objects may also contain the public exponent attribute, allowing us
    /// to derive the public key from the private key object.
    fn get_public_key_pem(&self, uri: Option<&str>) -> anyhow::Result<String>;

    fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>>;

    /// Generate a new keypair, saving the private key on the token and returning the public key as PEM.
    fn create_key(&self, request: CreateKeyRequest) -> anyhow::Result<CreateKeyResponse>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChooseSchemeRequest {
    pub offered: Vec<SignatureScheme>,
    pub uri: Option<String>,
    pub pin: Option<SecretString>,
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
    pub pin: Option<SecretString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignRequestWithSigScheme {
    pub to_sign: Vec<u8>,
    pub uri: Option<String>,
    pub sigscheme: Option<SigScheme>,
    pub pin: Option<SecretString>,
}

/// A secret string that should not be printed by accident.
///
/// Rolling our own type because `secrecy::SecretString` doesn't impl Serialize,
/// and we don't need eager zeroizing, we only need to make sure not to print
/// the value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(string: String) -> Self {
        Self(string)
    }

    pub fn expose(self) -> String {
        self.0
    }
}

impl From<SecretString> for AuthPin {
    fn from(value: SecretString) -> Self {
        AuthPin::new(value.0.into())
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        SecretString::new(value)
    }
}

impl Display for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignResponse(pub Vec<u8>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateKeyRequest {
    pub uri: String,
    pub params: CreateKeyParams,
    pub pin: Option<SecretString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateKeyResponse {
    pub pem: String,
    pub uri: String,
}

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
