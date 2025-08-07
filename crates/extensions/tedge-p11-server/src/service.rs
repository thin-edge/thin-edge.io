use crate::pkcs11::SigScheme;
use serde::Deserialize;
use serde::Serialize;

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
