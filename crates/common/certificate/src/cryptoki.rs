//! Signing self-signed certs and CSRs using private keys stored on a PKCS #11 cryptographic tokens,
//! accessible via Cryptoki.

use crate::CertificateError;
use crate::PemCertificate;
use crate::SignatureAlgorithm;
use anyhow::Context;
use camino::Utf8Path;
use tedge_p11_server::service::ChooseSchemeRequest;
use tedge_p11_server::CryptokiConfig;
use tracing::trace;
use x509_parser::public_key::PublicKey;

/// A key pair using a remote private key.
///
/// To generate a CSR we need:
/// - the public key, because the public key is a part of the certificate (subject public key info)
/// - the private key, to sign the CSR to prove that the public key is ours
///
/// With private key in the HSM, we can't access its private parts, but we can still use it to sign.
/// For the public key, instead of deriving it from the private key, which needs some additions to
/// our PKCS11 code, we can just reuse the SPKI section from an existing certificate if we have it,
/// i.e. we're renewing and not getting a brand new cert.
///
/// An alternative, looking at how it's done in gnutls (_pkcs11_privkey_get_pubkey and
/// pkcs11_read_pubkey functions), seems to be:
/// - if the key is RSA, a public key can be trivially derived from the public properties of PKCS11
///   private key object
/// - for EC key, it should also be possible to derive public from private
/// - if that fails, a public key object may also be present on the token
#[derive(Debug, Clone)]
pub struct RemoteKeyPair {
    pub cryptoki_config: CryptokiConfig,
    pub public_key_raw: Vec<u8>,
    pub(crate) algorithm: SignatureAlgorithm,
}

impl rcgen::PublicKeyData for RemoteKeyPair {
    fn der_bytes(&self) -> &[u8] {
        &self.public_key_raw
    }

    fn algorithm(&self) -> &'static rcgen::SignatureAlgorithm {
        self.algorithm.into()
    }
}

impl rcgen::SigningKey for RemoteKeyPair {
    fn sign(&self, msg: &[u8]) -> Result<Vec<u8>, rcgen::Error> {
        // the error here is not PEM-related, but we need to return a foreign error type, and there
        // are no other better variants that could let us return context, so we'll have to use this
        // until `rcgen::Error::RemoteKeyError` can take a parameter
        trace!(?self.cryptoki_config, msg = %String::from_utf8_lossy(msg), "sign");
        let signer = tedge_p11_server::signing_key(self.cryptoki_config.clone())
            .map_err(|e| rcgen::Error::PemError(e.to_string()))?;
        signer
            .sign2(msg, self.algorithm.into())
            .map_err(|e| rcgen::Error::PemError(e.to_string()))
    }
}

impl RemoteKeyPair {
    fn from_cryptoki_and_existing_cert(
        cryptoki_config: CryptokiConfig,
        current_cert: &Utf8Path,
    ) -> Result<Self, CertificateError> {
        let cert = PemCertificate::from_pem_file(current_cert)?;
        let cert = PemCertificate::extract_certificate(&cert.pem)?;
        let public_key_raw = cert.public_key().subject_public_key.data.to_vec();

        // map public key to signature identifier (some keys support many types of signatures, on
        // our side we only do P256/P384/RSA2048 with a single type of signature each)
        // for P256/P384, former has only SHA256 and latter only SHA384 signature, so no questions there
        // but for RSA, AFAIK we can use SHA256 with all of them. RSA_PSS also isn't supported by
        // rcgen, but we're free to use regular RSA_PKCS1_SHA256
        let public_key = cert
            .public_key()
            .parsed()
            .context("Failed to read public key from the certificate")?;
        let algorithm = match public_key {
            PublicKey::EC(ec) => match ec.key_size() {
                256 => SignatureAlgorithm::EcdsaP256Sha256,
                384 => SignatureAlgorithm::EcdsaP384Sha384,
                // P521 (size 528 reported by key_size() is not yet supported by rcgen)
                // https://github.com/rustls/rcgen/issues/60
                _ => {
                    return Err(anyhow::anyhow!("Unsupported public key. Only P256/P384/RSA2048/RSA3072/RSA4096 are supported for certificate renewal").into());
                }
            },
            PublicKey::RSA(_) => SignatureAlgorithm::RsaPkcs1Sha256,
            _ => return Err(anyhow::anyhow!("Unsupported public key. Only P256/P384/RSA2048/RSA3072/RSA4096 are supported for certificate renewal").into())
        };

        Ok(RemoteKeyPair {
            cryptoki_config,
            public_key_raw,
            algorithm,
        })
    }

    pub fn from_cryptoki(
        cryptoki_config: CryptokiConfig,
        current_cert: Option<&Utf8Path>,
    ) -> Result<Self, CertificateError> {
        let cryptoki = tedge_p11_server::tedge_p11_service(cryptoki_config.clone())?;

        let pubkey_pem = cryptoki.get_public_key_pem(None);
        let pubkey_pem = match pubkey_pem {
            Ok(p) => p,
            Err(err) => {
                let e = format!("{err:#}");
                if e.contains("Failed to parse the received frame") {
                    // server doesn't understand the request, too old, fallback to the older method of just resigning existing certificate
                    let Some(current_cert) = current_cert else {
                        return Err(CertificateError::Other(
                            anyhow::anyhow!("tedge-p11-server can only renew existing certificates but there's no existing certificate; upgrade tedge-p11-server or generate a self-signed certificate first")));
                    };
                    return Self::from_cryptoki_and_existing_cert(cryptoki_config, current_cert);
                }
                return Err(err.into());
            }
        };

        let public_key = pem::parse(&pubkey_pem).unwrap();
        let public_key_raw = public_key.into_contents();

        let signature_algorithm = cryptoki.choose_scheme(ChooseSchemeRequest {
            offered: vec![
                tedge_p11_server::service::SignatureScheme(
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                ),
                tedge_p11_server::service::SignatureScheme(
                    rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                ),
                tedge_p11_server::service::SignatureScheme(
                    rustls::SignatureScheme::RSA_PKCS1_SHA256,
                ),
            ],
            uri: None,
            // pin will be applied anyway by the client
            pin: None,
        })?;
        let signature_algorithm = signature_algorithm
            .scheme
            .context("No supported scheme found")?
            .0;

        let algorithm = match signature_algorithm {
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256 => SignatureAlgorithm::EcdsaP256Sha256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384 => SignatureAlgorithm::EcdsaP384Sha384,
            rustls::SignatureScheme::RSA_PKCS1_SHA256 => SignatureAlgorithm::RsaPkcs1Sha256,
            _ => return Err(anyhow::anyhow!("Unsupported signature scheme").into()),
        };

        Ok(RemoteKeyPair {
            cryptoki_config,
            public_key_raw,
            algorithm,
        })
    }
}
