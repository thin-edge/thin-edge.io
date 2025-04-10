//! rustls connector for PKCS#11 devices.
//!
//! Reference:
//! - thin-edge: docs/src/references/hsm-support.md
//! - PKCS#11: https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html

pub use cryptoki::types::AuthPin;

use anyhow::Context;
use asn1_rs::ToDer;
use camino::Utf8PathBuf;
use cryptoki::context::CInitializeArgs;
use cryptoki::context::Pkcs11;
use cryptoki::error::Error;
use cryptoki::mechanism::Mechanism;
use cryptoki::object::Attribute;
use cryptoki::object::AttributeType;
use cryptoki::object::KeyType;
use cryptoki::session::Session;
use cryptoki::session::UserType;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use rustls::SignatureAlgorithm;
use rustls::SignatureScheme;
use tracing::debug;
use tracing::trace;
use tracing::warn;

use std::fmt::Debug;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct CryptokiConfigDirect {
    pub module_path: Utf8PathBuf,
    pub pin: AuthPin,
    pub serial: Option<Arc<str>>,
    pub uri: Option<Arc<str>>,
}

impl Debug for CryptokiConfigDirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CryptokiConfigDirect")
            .field("module_path", &self.module_path)
            .field("pin", &"[REDACTED]")
            .field("serial", &self.serial)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Cryptoki {
    context: Pkcs11,
    config: CryptokiConfigDirect,
}

impl Cryptoki {
    pub fn new(config: CryptokiConfigDirect) -> anyhow::Result<Self> {
        debug!(module_path = %config.module_path, "Loading PKCS#11 module");
        // can fail with Pkcs11(GeneralError, GetFunctionList) if P11_KIT_SERVER_ADDRESS is wrong
        let pkcs11client = match Pkcs11::new(&config.module_path) {
            Ok(p) => p,
            // i want to get inner error but i don't know if there is a better way to do this
            Err(Error::LibraryLoading(e)) => {
                return Err(e).context("Failed to load PKCS#11 dynamic object");
            }
            Err(e) => {
                return Err(e).context("Failed to load PKCS#11 dynamic object");
            }
        };

        pkcs11client.initialize(CInitializeArgs::OsThreads)?;

        Ok(Self {
            context: pkcs11client,
            config,
        })
    }

    pub fn signing_key(&self) -> anyhow::Result<Pkcs11SigningKey> {
        let CryptokiConfigDirect { pin, uri, .. } = &self.config;

        let uri_attributes = uri
            .as_deref()
            .map(|uri| uri::Pkcs11Uri::parse(uri).context("Failed to parse PKCS #11 URI"))
            .transpose()?;

        let wanted_label = uri_attributes.as_ref().and_then(|u| u.token.as_ref());
        let wanted_serial = uri_attributes.as_ref().and_then(|u| u.serial.as_ref());

        let slots_with_tokens = self.context.get_slots_with_token()?;
        let tokens: Result<Vec<_>, _> = slots_with_tokens
            .iter()
            .map(|s| {
                self.context
                    .get_token_info(*s)
                    .context("Failed to get slot info")
            })
            .collect();
        let tokens = tokens?;

        // if token/serial attributes are passed, find a token that has these attributes, otherwise any token will do
        let mut tokens = slots_with_tokens
            .into_iter()
            .zip(tokens)
            .filter(|(_, t)| wanted_label.is_none() || wanted_label.is_some_and(|l| t.label() == l))
            .filter(|(_, t)| {
                wanted_serial.is_none() || wanted_serial.is_some_and(|s| t.serial_number() == s)
            });
        let (slot, _) = tokens
            .next()
            .context("Didn't find a slot to use. The device may be disconnected.")?;

        let slot_info = self.context.get_slot_info(slot)?;
        let token_info = self.context.get_token_info(slot)?;
        debug!(?slot_info, ?token_info, "Selected slot");

        let session = self.context.open_ro_session(slot)?;
        session.login(UserType::User, Some(pin))?;
        let session_info = session.get_session_info()?;
        debug!(?session_info, "Opened a readonly session");

        // get the signing key
        let key = match &uri_attributes {
            Some(uri::Pkcs11Uri { id, object, .. }) if id.is_some() || object.is_some() => {
                let key = Self::find_key_by_attributes(&uri_attributes.unwrap(), &session)?;
                let key_type = session
                    .get_attributes(key, &[AttributeType::KeyType])?
                    .into_iter()
                    .next()
                    .context("no keytype attribute")?;
                let Attribute::KeyType(keytype) = key_type else {
                    anyhow::bail!("can't get key type");
                };
                let pkcs11 = PKCS11 {
                    session: Arc::new(Mutex::new(session)),
                };
                match keytype {
                    KeyType::EC => Pkcs11SigningKey::Ecdsa(ECSigningKey { pkcs11 }),
                    KeyType::RSA => Pkcs11SigningKey::Rsa(RSASigningKey { pkcs11 }),
                    _ => anyhow::bail!("unsupported key type"),
                }
            }
            _ => {
                let pkcs11 = PKCS11 {
                    session: Arc::new(Mutex::new(session)),
                };
                let key_type = get_key_type(pkcs11.clone()).context("Failed to read key")?;
                trace!("Key Type: {:?}", key_type.to_string());
                match key_type {
                    KeyType::EC => Pkcs11SigningKey::Ecdsa(ECSigningKey { pkcs11 }),
                    KeyType::RSA => Pkcs11SigningKey::Rsa(RSASigningKey { pkcs11 }),
                    _ => {
                        anyhow::bail!("Unsupported key type. Only EC and RSA keys are supported");
                    }
                }
            }
        };

        Ok(key)
    }

    fn find_key_by_attributes(
        uri: &uri::Pkcs11Uri,
        session: &Session,
    ) -> anyhow::Result<cryptoki::object::ObjectHandle> {
        let mut key_template = vec![
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sign(true),
        ];
        if let Some(object) = &uri.object {
            key_template.push(Attribute::Label(object.as_bytes().to_vec()));
        }
        if let Some(id) = &uri.id {
            key_template.push(Attribute::Id(id.clone()));
        }

        trace!(?key_template, "Finding a key");

        let mut keys = session
            .find_objects(&key_template)
            .context("Failed to find private key objects")?
            .into_iter();

        let key = keys.next().context("Failed to find a private key")?;
        if keys.len() > 0 {
            warn!("Multiple keys were found. If the wrong one was chosen, please use a URI that uniquely identifies a key.")
        }

        Ok(key)
    }
}

#[derive(Debug)]
pub enum Pkcs11SigningKey {
    Rsa(RSASigningKey),
    Ecdsa(ECSigningKey),
}

impl SigningKey for Pkcs11SigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        match self {
            Self::Ecdsa(e) => e.choose_scheme(offered),
            Self::Rsa(r) => r.choose_scheme(offered),
        }
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        match self {
            Self::Ecdsa(e) => e.algorithm(),
            Self::Rsa(r) => r.algorithm(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PKCS11 {
    pub session: Arc<Mutex<Session>>,
}

#[derive(Debug)]
pub struct PkcsSigner {
    pub pkcs11: PKCS11,
    pub scheme: SignatureScheme,
}

impl PkcsSigner {
    pub fn from_session(session: PKCS11) -> Self {
        Self {
            pkcs11: session,
            scheme: SignatureScheme::ECDSA_NISTP256_SHA256,
        }
    }

    fn get_mechanism(&self) -> Result<Mechanism, rustls::Error> {
        trace!("Getting mechanism from chosen scheme: {:?}", self.scheme);
        match self.scheme {
            SignatureScheme::ED25519 => Ok(Mechanism::Eddsa),
            SignatureScheme::ECDSA_NISTP256_SHA256 => Ok(Mechanism::EcdsaSha256),
            SignatureScheme::ECDSA_NISTP384_SHA384 => Ok(Mechanism::EcdsaSha384),
            SignatureScheme::ECDSA_NISTP521_SHA512 => Ok(Mechanism::EcdsaSha512),
            SignatureScheme::RSA_PKCS1_SHA1 => Ok(Mechanism::Sha1RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA256 => Ok(Mechanism::Sha256RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA384 => Ok(Mechanism::Sha384RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA512 => Ok(Mechanism::Sha512RsaPkcs),
            _ => Err(rustls::Error::General(
                "Unsupported signature scheme".to_owned(),
            )),
        }
    }

    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        let session = self.pkcs11.session.lock().unwrap();

        let key_template = vec![
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sign(true),
        ];

        let key = session
            .find_objects(&key_template)
            .map_err(|_| rustls::Error::General("pkcs11: Failed to find objects".to_owned()))?
            .into_iter()
            .next()
            .ok_or_else(|| rustls::Error::General("pkcs11: No signing key found".to_owned()))?;
        debug!(?key, "selected key to sign");

        let mechanism = self.get_mechanism()?;

        let (mechanism, digest_mechanism) = match mechanism {
            Mechanism::EcdsaSha256 => (Mechanism::Ecdsa, Some(Mechanism::Sha256)),
            Mechanism::EcdsaSha384 => (Mechanism::Ecdsa, Some(Mechanism::Sha384)),
            Mechanism::EcdsaSha512 => (Mechanism::Ecdsa, Some(Mechanism::Sha512)),
            Mechanism::Sha1RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha1)),
            Mechanism::Sha256RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha256)),
            Mechanism::Sha384RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha384)),
            Mechanism::Sha512RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha512)),
            _ => {
                warn!(?mechanism, "Unsupported mechanism, trying it out anyway.");
                (Mechanism::Ecdsa, Some(Mechanism::Sha256))
            }
        };

        let direct_sign = digest_mechanism.is_none();

        trace!(input_message = %String::from_utf8_lossy(message), len=message.len(), ?mechanism, direct_sign);

        let digest;
        let to_sign = if direct_sign {
            message
        } else {
            digest = session
                .digest(&digest_mechanism.unwrap(), message)
                .map_err(|err| {
                    rustls::Error::General(format!("pkcs11: Failed to digest message: {:?}", err))
                })?;
            &digest
        };

        let signature_raw = session.sign(&mechanism, key, to_sign).map_err(|err| {
            rustls::Error::General(format!("pkcs11: Failed to sign message: {:?}", err))
        })?;

        // Split raw signature into r and s values (assuming 32 bytes each)
        trace!("Signature (raw) len={:?}", signature_raw.len());
        let r_bytes = &signature_raw[0..32];
        let s_bytes = &signature_raw[32..];
        let signature_asn1 = format_asn1_ecdsa_signature(r_bytes, s_bytes).map_err(|err| {
            rustls::Error::General(format!("pkcs11: Failed to format signature: {:?}", err))
        })?;
        trace!(
            "Encoded ASN.1 Signature: len={:?} {:?}",
            signature_asn1.len(),
            signature_asn1
        );
        Ok(signature_asn1)
    }
}

impl Signer for PkcsSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        Self::sign(self, message)
    }

    fn scheme(&self) -> SignatureScheme {
        trace!("Using Signature scheme: {:?}", self.scheme.as_str());
        self.scheme
    }
}

#[derive(Debug)]
pub struct ECSigningKey {
    pub pkcs11: PKCS11,
}

impl ECSigningKey {
    fn supported_schemes(&self) -> &[SignatureScheme] {
        &[
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
        ]
    }
}

impl SigningKey for ECSigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        trace!("Offered signature schemes. offered={:?}", offered);
        let supported = self.supported_schemes();
        for scheme in offered {
            if supported.contains(scheme) {
                trace!("Matching scheme: {:?}", scheme.as_str());
                return Some(Box::new(PkcsSigner {
                    pkcs11: self.pkcs11.clone(),
                    scheme: *scheme,
                }));
            }
        }
        None
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

#[derive(Debug)]
pub struct RSASigningKey {
    pub pkcs11: PKCS11,
}

impl RSASigningKey {
    fn supported_schemes(&self) -> &[SignatureScheme] {
        &[
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

impl SigningKey for RSASigningKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        debug!("Offered signature schemes. offered={:?}", offered);
        let supported = self.supported_schemes();
        for scheme in offered {
            if supported.contains(scheme) {
                debug!("Matching scheme: {:?}", scheme.as_str());
                return Some(Box::new(PkcsSigner {
                    pkcs11: self.pkcs11.clone(),
                    scheme: *scheme,
                }));
            }
        }
        None
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::RSA
    }
}

fn get_key_type(pkcs11: PKCS11) -> anyhow::Result<KeyType> {
    let session = pkcs11.session.lock().unwrap();

    let key_template = vec![
        Attribute::Token(true),
        Attribute::Private(true),
        Attribute::Sign(true),
    ];

    trace!(?key_template, "Finding a key");
    let mut keys = session
        .find_objects(&key_template)
        .context("Failed to find private key objects")?
        .into_iter();

    let key = keys.next().context("Failed to find a private key")?;
    if keys.len() > 0 {
        warn!("Multiple keys were found. If the wrong one was chosen, please use a URI that uniquely identifies a key.")
    }

    let info = session
        .get_attributes(
            key,
            &[
                AttributeType::KeyType,
                AttributeType::Token,
                AttributeType::Private,
                AttributeType::Sign,
            ],
        )
        .context("failed to get key attributes")?;

    let mut key_type = KeyType::EC;
    let Attribute::KeyType(returned_key_type) = info
        .iter()
        .find(|a| a.attribute_type() == AttributeType::KeyType)
        .unwrap()
    else {
        return Ok(key_type);
    };
    key_type = *returned_key_type;

    Ok(key_type)
}

fn format_asn1_ecdsa_signature(r_bytes: &[u8], s_bytes: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    let mut writer = Vec::new();

    write_asn1_integer(&mut writer, r_bytes);

    write_asn1_integer(&mut writer, s_bytes);

    let seq = asn1_rs::Sequence::new(writer.into());
    let b = seq.to_der_vec().unwrap();
    Ok(b)
}

fn write_asn1_integer(writer: &mut dyn std::io::Write, b: &[u8]) {
    let mut i = asn1_rs::BigInt::from_signed_bytes_be(b);
    if i.sign() == asn1_rs::Sign::Minus {
        // Prepend a most significant zero byte if value < 0
        let mut positive = b.to_vec();
        positive.insert(0, 0);

        i = asn1_rs::BigInt::from_signed_bytes_be(&positive);
    }
    let i = i.to_signed_bytes_be();
    let i = asn1_rs::Integer::new(&i);
    let _ = i.write_der(writer);
}

pub mod uri {
    use std::borrow::Cow;
    use std::collections::HashMap;

    /// Attributes decoded from a PKCS #11 URL.
    ///
    /// Attributes only relevant to us shall be put into fields and the rest is in `other` hashmap.
    ///
    /// https://www.rfc-editor.org/rfc/rfc7512.html
    #[derive(Debug, Clone)]
    pub struct Pkcs11Uri<'a> {
        pub token: Option<Cow<'a, str>>,
        pub serial: Option<Cow<'a, str>>,
        pub id: Option<Vec<u8>>,
        pub object: Option<Cow<'a, str>>,
        pub other: HashMap<&'a str, Cow<'a, str>>,
    }

    impl<'a> Pkcs11Uri<'a> {
        pub fn parse(uri: &'a str) -> anyhow::Result<Self> {
            let path = uri
                .strip_prefix("pkcs11:")
                .ok_or_else(|| anyhow::anyhow!("missing PKCS #11 URI scheme"))?;

            // split of the query component
            let path = path.split_once('?').map(|(l, _)| l).unwrap_or(path);

            // parse attributes, duplicate attributes are an error (RFC section 2.3)
            let pairs_iter = path.split(';').filter_map(|pair| pair.split_once('='));
            let mut pairs: HashMap<&str, &str> = HashMap::new();
            for (k, v) in pairs_iter {
                let prev_value = pairs.insert(k, v);
                if prev_value.is_some() {
                    anyhow::bail!("PKCS#11 URI contains duplicate attribute ({k})");
                }
            }

            let token = pairs
                .remove("token")
                .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());
            let serial = pairs
                .remove("serial")
                .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());
            let object = pairs
                .remove("object")
                .map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy());

            let id: Option<Vec<u8>> = pairs
                .remove("id")
                .map(|id| percent_encoding::percent_decode_str(id).collect());

            let other = pairs
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        percent_encoding::percent_decode_str(v).decode_utf8_lossy(),
                    )
                })
                .collect();

            Ok(Self {
                token,
                serial,
                id,
                object,
                other,
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decodes_valid_pkcs11_uri() {
            // test input URIs taken from RFC examples section and combined with properties we actually use
            // https://www.rfc-editor.org/rfc/rfc7512.html#section-3
            let input = "pkcs11:token=The%20Software%20PKCS%2311%20Softtoken;\
            manufacturer=Snake%20Oil,%20Inc.;\
            model=1.0;\
            object=my-certificate;\
            type=cert;\
            id=%69%95%3E%5C%F4%BD%EC%91;\
            serial=\
            ?pin-source=file:/etc/token_pin";

            let attributes = Pkcs11Uri::parse(input).unwrap();

            assert_eq!(attributes.token.unwrap(), "The Software PKCS#11 Softtoken");
            assert_eq!(
                attributes.other.get("manufacturer").unwrap(),
                "Snake Oil, Inc."
            );
            assert_eq!(attributes.other.get("model").unwrap(), "1.0");
            assert_eq!(attributes.serial.unwrap(), "");
            assert_eq!(attributes.object.unwrap(), "my-certificate");
            assert_eq!(
                attributes.id,
                Some(vec![0x69, 0x95, 0x3e, 0x5c, 0xf4, 0xbd, 0xec, 0x91])
            );
        }

        #[test]
        fn fails_on_uris_with_duplicate_attributes() {
            let input = "pkcs11:token=my-token;token=my-token";
            let err = Pkcs11Uri::parse(input).unwrap_err();
            assert!(err
                .to_string()
                .contains("PKCS#11 URI contains duplicate attribute (token)"));
        }

        #[test]
        fn fails_on_uris_with_invalid_scheme() {
            let input = "not a pkcs#11 uri";
            let err = Pkcs11Uri::parse(input).unwrap_err();
            assert!(err.to_string().contains("missing PKCS #11 URI scheme"));
        }
    }
}
