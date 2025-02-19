//! rustls connector for PKCS#11 devices.
//!
//! Reference:
//! - thin-edge: docs/src/references/hsm-support.md
//! - PKCS#11: https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html

use anyhow::Context;
use asn1_rs::ToDer;
use base64::Engine;
use cryptoki::context::CInitializeArgs;
use cryptoki::context::Pkcs11;
use cryptoki::mechanism::Mechanism;
use cryptoki::object::Attribute;
use cryptoki::object::AttributeType;
use cryptoki::object::KeyType;
use cryptoki::session::Session;
use cryptoki::session::UserType;
use cryptoki::types::AuthPin;
use rustls::client::ResolvesClientCert;
use rustls::pki_types::CertificateDer;
use rustls::sign::CertifiedKey;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use rustls::SignatureAlgorithm;
use rustls::SignatureScheme;
use tracing::trace;

use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::debug;
use tracing::error;
use tracing::warn;

use super::CryptokiConfig;

#[derive(Debug)]
pub enum Pkcs11SigningKey {
    Rsa(RSASigningKey),
    Ecdsa(ECSigningKey),
}

impl Pkcs11SigningKey {
    pub fn from_cryptoki_config(
        cryptoki_config: CryptokiConfig,
    ) -> anyhow::Result<Pkcs11SigningKey> {
        let CryptokiConfig {
            module_path,
            pin,
            // TODO(marcel): select modules by serial if multiple are connected
            serial: _,
        } = cryptoki_config;

        debug!(%module_path, "Loading PKCS#11 module");
        // can fail with Pkcs11(GeneralError, GetFunctionList) if P11_KIT_SERVER_ADDRESS is wrong
        let pkcs11client = Pkcs11::new(module_path)?;
        pkcs11client.initialize(CInitializeArgs::OsThreads)?;

        // Select first available slot. If it's not the one we want, the token
        // used in p11-kit-server should be adjusted.
        let slot = pkcs11client
            .get_slots_with_token()?
            .into_iter()
            .next()
            .context("Didn't find a slot to use. The device may be disconnected.")?;
        let slot_info = pkcs11client.get_slot_info(slot)?;
        let token_info = pkcs11client.get_token_info(slot)?;
        debug!(?slot_info, ?token_info, "Selected slot");

        debug!(%pin, "Attempting to login to PKCS#11 module");
        let session = pkcs11client.open_ro_session(slot)?;
        session.login(UserType::User, Some(&AuthPin::new(pin.deref().into())))?;
        let session_info = session.get_session_info()?;
        debug!(?session_info, "Opened a readonly session");

        let pkcs11 = PKCS11 {
            session: Arc::new(Mutex::new(session)),
        };

        let key_type = get_key_type(pkcs11.clone()).context("Failed to read key")?;
        trace!("Key Type: {:?}", key_type.to_string());
        let key = match key_type {
            KeyType::EC => Pkcs11SigningKey::Ecdsa(ECSigningKey { pkcs11 }),
            KeyType::RSA => Pkcs11SigningKey::Rsa(RSASigningKey { pkcs11 }),
            _ => {
                panic!("Unsupported key type. Only EC and RSA keys are supported");
            }
        };

        Ok(key)
    }
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
struct PKCS11 {
    session: Arc<Mutex<Session>>,
}

#[derive(Debug)]
struct PkcsSigner {
    pkcs11: PKCS11,
    scheme: SignatureScheme,
}

impl PkcsSigner {
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
}

impl Signer for PkcsSigner {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
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

    fn scheme(&self) -> SignatureScheme {
        trace!("Using Signature scheme: {:?}", self.scheme.as_str());
        self.scheme
    }
}

#[derive(Debug)]
pub struct Pkcs11Resolver {
    pub chain: Vec<CertificateDer<'static>>,
    pub signing_key: Arc<dyn SigningKey>,
}

impl ResolvesClientCert for Pkcs11Resolver {
    fn resolve(
        &self,
        _acceptable_issuers: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        Some(Arc::new(CertifiedKey {
            cert: self.chain.clone(),
            key: self.signing_key.clone(),
            ocsp: None,
        }))
    }

    fn has_certs(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct ECSigningKey {
    pkcs11: PKCS11,
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
        error!(
            "Could not find a matching signing scheme. offered={:?}",
            offered
        );
        None
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}

#[derive(Debug)]
pub struct RSASigningKey {
    pkcs11: PKCS11,
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
        debug!(
            "Could not find a matching signing scheme. offered={:?}",
            offered
        );
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
    let key = session
        .find_objects(&key_template)
        .context("Failed to find object")?
        .into_iter()
        .next()
        .context("Failed to find a key")?;

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
    use base64::prelude::BASE64_STANDARD_NO_PAD;
    let mut writer = Vec::new();

    write_asn1_integer(&mut writer, r_bytes);

    write_asn1_integer(&mut writer, s_bytes);

    let seq = asn1_rs::Sequence::new(writer.into());
    let b = seq.to_der_vec().unwrap();
    trace!("Encoded ASN.1 Der: {:?}", BASE64_STANDARD_NO_PAD.encode(&b));
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
