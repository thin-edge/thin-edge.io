//! Example of how to configure rumqttd to connect to a server using TLS and authentication.
//! Source https://github.com/leonardodepaula/Cryptoki-TLS
//! https://github.com/rustls/rustls-cng/blob/dev/src/signer.rs

use asn1_rs::ToDer;
use base64::Engine;
use cryptoki::context::CInitializeArgs;
use cryptoki::context::Pkcs11;
use cryptoki::mechanism::rsa::PkcsMgfType;
use cryptoki::mechanism::rsa::PkcsPssParams;
use cryptoki::mechanism::Mechanism;
use cryptoki::mechanism::MechanismType;
use cryptoki::object::Attribute;
use cryptoki::object::AttributeType;
use cryptoki::object::CertificateType;
use cryptoki::object::KeyType;
use cryptoki::object::ObjectClass;
use cryptoki::session::Session;
use cryptoki::session::UserType;
use cryptoki::types::AuthPin;
use rustls::client::ResolvesClientCert;
use rustls::pki_types::CertificateDer;
use rustls::sign::CertifiedKey;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use rustls::Error as RusTLSError;
use rustls::SignatureAlgorithm;
use rustls::SignatureScheme;

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
            // TODO(marcel): select moduly by serial if multiple are connected
            serial,
        } = cryptoki_config;

        // Alternative module: /opt/homebrew/lib/pkcs11/opensc-pkcs11.so
        // let pkcs11module = std::env::var("PKCS11_MODULE");
        // let pkcs11module = pkcs11module.as_deref().unwrap_or(PCKS11_MODULE_PATH);

        debug!(%module_path, "Loading PKCS#11 module");
        let pkcs11client = Pkcs11::new(module_path)?;
        pkcs11client.initialize(CInitializeArgs::OsThreads)?;

        debug!(%pin, "Attempting to login to PKCS#11 module");
        let slot = pkcs11client.get_slots_with_token()?.remove(0);
        let session = pkcs11client.open_ro_session(slot)?;
        session.login(UserType::User, Some(&AuthPin::new(pin.deref().into())))?;

        // Debug
        let search = vec![
            Attribute::Class(ObjectClass::CERTIFICATE),
            Attribute::CertificateType(CertificateType::X_509),
        ];
        for handle in session.find_objects(&search)? {
            // each cert: get the "value" which will be the raw certificate data
            for value in session.get_attributes(handle, &[AttributeType::SerialNumber])? {
                if let Attribute::Value(value) = value {
                    match String::from_utf8(value) {
                        Ok(path) => println!("Certificate value: {:?}", path),
                        Err(e) => println!("Invalid UTF-8 sequence: {}", e),
                    };
                }
            }
        }

        let pkcs11 = PKCS11 {
            session: Arc::new(Mutex::new(session)),
        };
        let key_type = get_key_type(pkcs11.clone());
        debug!("Key Type: {:?}", key_type.to_string());
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
    fn get_mechanism(&self) -> anyhow::Result<Mechanism, RusTLSError> {
        debug!("Getting mechanism from chosen scheme: {:?}", self.scheme);
        match self.scheme {
            SignatureScheme::ED25519 => Ok(Mechanism::Eddsa),
            SignatureScheme::ECDSA_NISTP256_SHA256 => Ok(Mechanism::EcdsaSha256),
            SignatureScheme::ECDSA_NISTP384_SHA384 => Ok(Mechanism::EcdsaSha384),
            SignatureScheme::ECDSA_NISTP521_SHA512 => Ok(Mechanism::EcdsaSha512),
            SignatureScheme::RSA_PKCS1_SHA1 => Ok(Mechanism::Sha1RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA256 => Ok(Mechanism::Sha256RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA384 => Ok(Mechanism::Sha384RsaPkcs),
            SignatureScheme::RSA_PKCS1_SHA512 => Ok(Mechanism::Sha512RsaPkcs),
            SignatureScheme::RSA_PSS_SHA256 => {
                let params = PkcsPssParams {
                    hash_alg: MechanismType::SHA256_RSA_PKCS,
                    mgf: PkcsMgfType::MGF1_SHA256,
                    s_len: 32.into(),
                };
                Ok(Mechanism::Sha256RsaPkcsPss(params))
            }
            SignatureScheme::RSA_PSS_SHA384 => {
                let params = PkcsPssParams {
                    hash_alg: MechanismType::SHA384_RSA_PKCS,
                    mgf: PkcsMgfType::MGF1_SHA384,
                    s_len: 48.into(),
                };
                Ok(Mechanism::Sha384RsaPkcsPss(params))
            }
            SignatureScheme::RSA_PSS_SHA512 => {
                let params = PkcsPssParams {
                    hash_alg: MechanismType::SHA512_RSA_PKCS,
                    mgf: PkcsMgfType::MGF1_SHA512,
                    s_len: 64.into(),
                };
                Ok(Mechanism::Sha512RsaPkcsPss(params))
            }
            _ => Err(RusTLSError::General(
                "Unsupported signature scheme".to_owned(),
            )),
        }
    }
}

impl Signer for PkcsSigner {
    fn sign(&self, message: &[u8]) -> anyhow::Result<Vec<u8>, RusTLSError> {
        let session = self.pkcs11.session.lock().unwrap();

        let key_template = vec![
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sign(true),
        ];

        let key = session
            .find_objects(&key_template)
            .unwrap()
            .into_iter()
            .nth(0)
            .unwrap();

        let mechanism = self.get_mechanism().unwrap();

        let (mechanism, digest_mechanism) = match mechanism {
            Mechanism::EcdsaSha256 => (Mechanism::Ecdsa, Some(Mechanism::Sha256)),
            Mechanism::EcdsaSha384 => (Mechanism::Ecdsa, Some(Mechanism::Sha384)),
            Mechanism::EcdsaSha512 => (Mechanism::Ecdsa, Some(Mechanism::Sha512)),
            Mechanism::Sha1RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha1)),
            Mechanism::Sha256RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha256)),
            Mechanism::Sha384RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha384)),
            Mechanism::Sha512RsaPkcs => (Mechanism::RsaPkcs, Some(Mechanism::Sha512)),
            _ => {
                warn!("Warning: Unsupported mechanism, trying it out anyway. value={mechanism:?}");
                (Mechanism::Ecdsa, Some(Mechanism::Sha256))
            }
        };

        debug!(
            "Input message ({:?}): {:?}",
            mechanism,
            String::from_utf8_lossy(message)
        );

        let direct_sign = digest_mechanism.is_none();
        debug!("Direct sign: {direct_sign:?}");

        let signature_raw = if direct_sign {
            debug!(
                "Signing message (len={:?}):\n{:?}",
                message.len(),
                base64::prelude::BASE64_STANDARD_NO_PAD.encode(message)
            );

            match session.sign(&mechanism, key, message) {
                Ok(result) => result,
                Err(err) => {
                    error!("Failed to sign: {err:?}");
                    "".into()
                }
            }
        } else {
            let digest = session.digest(&digest_mechanism.unwrap(), message).unwrap();
            session.sign(&mechanism, key, &digest).unwrap()
        };

        // Split raw signature into r and s values (assuming 32 bytes each)
        debug!("Signature (raw) len={:?}", signature_raw.len());
        let r_bytes = signature_raw[0..32].to_vec();
        let s_bytes = signature_raw[32..].to_vec();
        let signature_asn1 = format_asn1_ecdsa_signature(&r_bytes, &s_bytes).unwrap();
        debug!(
            "Encoded ASN.1 Signature: len={:?} {:?}",
            signature_asn1.len(),
            signature_asn1
        );
        Ok(signature_asn1)
    }

    fn scheme(&self) -> SignatureScheme {
        debug!("Using Signature scheme: {:?}", self.scheme.as_str());
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
            SignatureScheme::RSA_PSS_SHA256,
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

fn get_key_type(pkcs11: PKCS11) -> KeyType {
    let session = pkcs11.session.lock().unwrap();

    let key_template = vec![
        Attribute::Token(true),
        Attribute::Private(true),
        Attribute::Sign(true),
    ];

    let key = session
        .find_objects(&key_template)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let info = session.get_attributes(key, &[AttributeType::KeyType]);
    let mut key_type = KeyType::EC;
    match info {
        Ok(value) => {
            for v in &value[0..value.len() - 1] {
                match v {
                    Attribute::KeyType(raw_value) => {
                        key_type = *raw_value;
                    }
                    _ => {
                        warn!("Could not read attribute value: {:?}", v);
                    }
                }
            }
        }
        Err(err) => warn!("Could not read value: {err:?}"),
    };

    key_type
}

fn format_asn1_ecdsa_signature(r_bytes: &[u8], s_bytes: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    use base64::prelude::BASE64_STANDARD_NO_PAD;
    let mut writer = Vec::new();

    write_asn1_integer(&mut writer, r_bytes);

    write_asn1_integer(&mut writer, s_bytes);

    let seq = asn1_rs::Sequence::new(writer.into());
    let b = seq.to_der_vec().unwrap();
    println!("Encoded ASN.1 Der: {:?}", BASE64_STANDARD_NO_PAD.encode(&b));
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
