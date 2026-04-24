use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use asn1_rs::BigInt;
use asn1_rs::Integer;
use asn1_rs::SequenceOf;
use asn1_rs::ToDer;
use cryptoki::mechanism::rsa::PkcsMgfType;
use cryptoki::mechanism::rsa::PkcsPssParams;
use cryptoki::mechanism::Mechanism;
use cryptoki::mechanism::MechanismType;
use cryptoki::object::ObjectHandle;
use cryptoki::session::Session;
use rustls::sign::Signer;
use rustls::sign::SigningKey;
use rustls::SignatureAlgorithm;
use rustls::SignatureScheme;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::trace;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct Pkcs11Signer {
    pub(super) session: Arc<Mutex<Session>>,
    pub(super) key: ObjectHandle,
    pub sigscheme: SigScheme,
    pub secondary_schemes: Vec<SigScheme>,
}

impl Pkcs11Signer {
    pub fn sign(
        &self,
        message: &[u8],
        sigscheme: Option<SigScheme>,
    ) -> Result<Vec<u8>, anyhow::Error> {
        let session = self.session.lock().unwrap();

        let sigscheme = sigscheme.unwrap_or(self.sigscheme);
        let mechanism = sigscheme.into();
        let (mechanism, digest_mechanism) = match mechanism {
            Mechanism::EcdsaSha256 => (Mechanism::Ecdsa, Some(Mechanism::Sha256)),
            Mechanism::EcdsaSha384 => (Mechanism::Ecdsa, Some(Mechanism::Sha384)),
            Mechanism::EcdsaSha512 => (Mechanism::Ecdsa, Some(Mechanism::Sha512)),
            Mechanism::Sha256RsaPkcs => (Mechanism::Sha256RsaPkcs, None),
            Mechanism::Sha384RsaPkcs => (Mechanism::Sha384RsaPkcs, None),
            Mechanism::Sha512RsaPkcs => (Mechanism::Sha512RsaPkcs, None),
            Mechanism::Sha256RsaPkcsPss(p) => (Mechanism::Sha256RsaPkcsPss(p), None),
            Mechanism::Sha384RsaPkcsPss(p) => (Mechanism::Sha384RsaPkcsPss(p), None),
            Mechanism::Sha512RsaPkcsPss(p) => (Mechanism::Sha512RsaPkcsPss(p), None),
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
                .context("pkcs11: Failed to digest message")?;
            &digest
        };

        trace!(?mechanism, "Session::sign");
        let signature_raw = session
            .sign(&mechanism, self.key, to_sign)
            .context("pkcs11: Failed to sign message")?;

        // Split raw signature into r and s values (assuming 32 bytes each)
        trace!("Signature (raw) len={:?}", signature_raw.len());
        let signature_asn1 = match mechanism {
            Mechanism::Ecdsa => {
                let size = signature_raw.len() / 2;
                let r_bytes = &signature_raw[0..size];
                let s_bytes = &signature_raw[size..];

                format_asn1_ecdsa_signature(r_bytes, s_bytes)
                    .context("pkcs11: Failed to format signature")?
            }

            _ => signature_raw,
        };
        trace!(
            "Encoded ASN.1 Signature: len={:?} {:?}",
            signature_asn1.len(),
            signature_asn1
        );
        Ok(signature_asn1)
    }
}

/// Currently supported signature schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SigScheme {
    EcdsaNistp256Sha256,
    EcdsaNistp384Sha384,
    EcdsaNistp521Sha512,
    RsaPssSha256,
    RsaPkcs1Sha256,
}

impl From<SigScheme> for rustls::SignatureScheme {
    fn from(value: SigScheme) -> Self {
        match value {
            SigScheme::EcdsaNistp256Sha256 => Self::ECDSA_NISTP256_SHA256,
            SigScheme::EcdsaNistp384Sha384 => Self::ECDSA_NISTP384_SHA384,
            SigScheme::EcdsaNistp521Sha512 => Self::ECDSA_NISTP521_SHA512,
            SigScheme::RsaPssSha256 => Self::RSA_PSS_SHA256,
            SigScheme::RsaPkcs1Sha256 => Self::RSA_PKCS1_SHA256,
        }
    }
}

impl From<SigScheme> for crate::service::SignatureScheme {
    fn from(value: SigScheme) -> Self {
        Self(rustls::SignatureScheme::from(value))
    }
}

impl From<SigScheme> for rustls::SignatureAlgorithm {
    fn from(value: SigScheme) -> Self {
        match value {
            SigScheme::EcdsaNistp256Sha256
            | SigScheme::EcdsaNistp384Sha384
            | SigScheme::EcdsaNistp521Sha512 => Self::ECDSA,
            SigScheme::RsaPssSha256 | SigScheme::RsaPkcs1Sha256 => Self::RSA,
        }
    }
}

impl From<SigScheme> for Mechanism<'_> {
    fn from(value: SigScheme) -> Self {
        match value {
            SigScheme::EcdsaNistp256Sha256 => Self::EcdsaSha256,
            SigScheme::EcdsaNistp384Sha384 => Self::EcdsaSha384,
            SigScheme::EcdsaNistp521Sha512 => Self::EcdsaSha512,
            SigScheme::RsaPkcs1Sha256 => Self::Sha256RsaPkcs,
            SigScheme::RsaPssSha256 => Mechanism::Sha256RsaPkcsPss(PkcsPssParams {
                hash_alg: MechanismType::SHA256,
                mgf: PkcsMgfType::MGF1_SHA256,
                // RFC8446 4.2.3: RSASSA-PSS PSS algorithms: [...] The length of
                // the Salt MUST be equal to the length of the digest algorithm
                // SHA256: 256 bits = 32 bytes
                s_len: 32.into(),
            }),
        }
    }
}

impl SigningKey for Pkcs11Signer {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        debug!("Offered signature schemes. offered={:?}", offered);
        let key_scheme = self.sigscheme.into();
        if offered.contains(&key_scheme) {
            debug!("Matching scheme: {key_scheme:?}");
            return Some(Box::new(self.clone()));
        }

        for scheme in &self.secondary_schemes {
            let key_scheme = (*scheme).into();
            if offered.contains(&key_scheme) {
                debug!("Matching scheme: {key_scheme:?}");
                let mut signer = self.clone();
                signer.sigscheme = *scheme;
                return Some(Box::new(signer));
            }
        }

        None
    }

    fn algorithm(&self) -> SignatureAlgorithm {
        self.sigscheme.into()
    }
}

impl Signer for Pkcs11Signer {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, rustls::Error> {
        Self::sign(self, message, Some(self.sigscheme))
            .map_err(|e| rustls::Error::General(e.to_string()))
    }

    fn scheme(&self) -> SignatureScheme {
        self.sigscheme.into()
    }
}

/// Formats the output of PKCS11 EC signature as an ASN.1 Ecdsa-Sig-Value.
///
/// This function takes the raw `r` and `s` byte slices and encodes them as ASN.1 INTEGERs,
/// then wraps them in an ASN.1 SEQUENCE, and finally serializes the structure to DER.
///
/// PKCS#11 EC signature operations typically return a raw concatenation of the `r` and `s` values,
/// each representing a big-endian positive integer of fixed length (depending on the curve).
/// However, most cryptographic protocols (including TLS and X.509) expect ECDSA signatures to be
/// encoded as an ASN.1 DER SEQUENCE of two INTEGERs, as described in RFC 3279 section 2.2.3.
///
/// - https://docs.oasis-open.org/pkcs11/pkcs11-curr/v3.0/os/pkcs11-curr-v3.0-os.html#_Toc30061178
/// - https://www.ietf.org/rfc/rfc3279#section-2.2.3
fn format_asn1_ecdsa_signature(r_bytes: &[u8], s_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let r = format_asn1_integer(r_bytes).to_signed_bytes_be();
    let r = Integer::new(&r);
    let s = format_asn1_integer(s_bytes).to_signed_bytes_be();
    let s = Integer::new(&s);

    let seq = SequenceOf::<Integer>::from_iter([r, s]);
    let seq_der = seq
        .to_der_vec()
        .context("Unexpected ASN.1 error when serializing Ecdsa-Sig-Value")?;
    Ok(seq_der)
}

fn format_asn1_integer(b: &[u8]) -> BigInt {
    let mut i = asn1_rs::BigInt::from_signed_bytes_be(b);
    if i.sign() == asn1_rs::Sign::Minus {
        // Prepend a most significant zero byte if value < 0
        let mut positive = b.to_vec();
        positive.insert(0, 0);

        i = asn1_rs::BigInt::from_signed_bytes_be(&positive);
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use asn1_rs::Any;
    use asn1_rs::FromDer;
    use asn1_rs::Integer;
    use asn1_rs::SequenceOf;

    #[test]
    fn test_format_asn1_ecdsa_signature_invalid_asn1() {
        // Use 32-byte r and s (as for P-256)
        let r = [0x01u8; 32];
        let s = [0xffu8; 32];

        let der = format_asn1_ecdsa_signature(&r, &s).expect("Should encode");

        // Try to parse as ASN.1 SEQUENCE of two INTEGERs
        let parsed = Any::from_der(&der);
        assert!(parsed.is_ok(), "Should parse as ASN.1");

        // Now check that the sequence contains exactly two INTEGERs
        let seq: SequenceOf<Integer> = SequenceOf::from_der(&der)
            .expect("Should parse as sequence")
            .1;
        assert_eq!(seq.len(), 2, "ASN.1 sequence should have two items");

        // make sure input is not misinterpreted as negative numbers
        assert_eq!(seq[0].as_bigint().to_bytes_be().1, r);
        assert_eq!(seq[1].as_bigint().to_bytes_be().1, s);
    }
}
