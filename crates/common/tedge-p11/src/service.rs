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

    /// Initialize a token in a slot so that it can be used to store keys.
    ///
    /// This performs the full PKCS #11 initialization sequence: `C_InitToken` (which sets the
    /// Security Officer PIN and the token label) followed by a Security Officer login and
    /// `C_InitPIN` (which sets the user PIN used by all other operations).
    ///
    /// If the target slot is not specified, the single slot holding an uninitialized token is
    /// selected automatically. The operation is idempotent: if a token with the requested label is
    /// already initialized with a user PIN, it is left untouched and its URI is returned.
    fn init_token(&self, request: InitTokenRequest) -> anyhow::Result<InitTokenResponse>;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitTokenRequest {
    /// Token label (CKA_LABEL) to assign to the initialized token.
    pub label: String,
    /// Security Officer PIN used by `C_InitToken`. If not set, the user PIN is used as the SO PIN.
    pub so_pin: Option<SecretString>,
    /// User PIN set on the token via `C_InitPIN`. If not set, the configured PIN is used.
    pub pin: Option<SecretString>,
    /// Slot id to initialize. If not set, the single slot with an uninitialized token is selected.
    pub slot: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitTokenResponse {
    /// URI identifying the initialized token.
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
