//! Interfaces with the PKCS#11 dynamic module using cryptoki crate.
//!
//! # `p11tool` quirks
//!
//! As `p11tool` was used during development to inspect and manipulate PKCS #11 token and may be used by users for the
//! same purpose, below is a description of some `p11tool` quirks and why some information it should display is missing.
//!
//! These quirks were observed with p11tool 3.8.9.
//!
//! ## Listing full algorithm types of private keys
//!
//! When displaying objects on the token, like this:
//!
//! ```sh
//! $ p11tool --login --set-pin=123456 --list-all "pkcs11:token=token1"
//! Object 0:
//!     URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=c4d820e1042971ea;token=token1;id=%6A%6C%5A%EA%8C%04%91%5C%67%77%AA%E2%B1%67%C7%C0%56%4E%97%01;object=p11tool-ec256;type=public
//!     Type: Public key (EC/ECDSA-SECP256R1)
//!     Label: p11tool-ec256
//!     Flags: CKA_WRAP/UNWRAP;
//!     ID: 6a:6c:5a:ea:8c:04:91:5c:67:77:aa:e2:b1:67:c7:c0:56:4e:97:01
//!
//! Object 1:
//!     URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=c4d820e1042971ea;token=token1;id=%43%2F%58%39%E5%33%30%B2%F9%E0%99%6E%FD%92%32%03%98%E7%72%7A;object=ec521;type=private
//!     Type: Private key (EC/ECDSA)
//!     Label: ec521
//!     Flags: CKA_WRAP/UNWRAP; CKA_PRIVATE; CKA_NEVER_EXTRACTABLE; CKA_SENSITIVE;
//!     ID: 43:2f:58:39:e5:33:30:b2:f9:e0:99:6e:fd:92:32:03:98:e7:72:7a
//!
//! Object 2:
//!     URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=c4d820e1042971ea;token=token1;id=%BB%DD%35%F4%26%51%C8%E2%ED%E9%1E%21%4E%59%29%5F%C1%75%FB%21;object=rsa3072;type=private
//!     Type: Private key (RSA-3072)
//!     Label: rsa3072
//!     Flags: CKA_WRAP/UNWRAP; CKA_PRIVATE; CKA_NEVER_EXTRACTABLE; CKA_SENSITIVE;
//!     ID: bb:dd:35:f4:26:51:c8:e2:ed:e9:1e:21:4e:59:29:5f:c1:75:fb:21
//!
//! Object 3:
//!     URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=c4d820e1042971ea;token=token1;id=%6A%6C%5A%EA%8C%04%91%5C%67%77%AA%E2%B1%67%C7%C0%56%4E%97%01;object=p11tool-ec256;type=private
//!     Type: Private key (EC/ECDSA-SECP256R1)
//!     Label: p11tool-ec256
//!     Flags: CKA_WRAP/UNWRAP; CKA_PRIVATE; CKA_NEVER_EXTRACTABLE; CKA_SENSITIVE;
//!     ID: 6a:6c:5a:ea:8c:04:91:5c:67:77:aa:e2:b1:67:c7:c0:56:4e:97:01
//!
//! Object 4:
//!     URL: pkcs11:model=SoftHSM%20v2;manufacturer=SoftHSM%20project;serial=c4d820e1042971ea;token=token1;id=%EC%60%3C%DF%10%E7%54%7D%EE%27%EE%B9%99%8A%D6%B1%45%C3%73%B8;type=public
//!     Type: Public key (EC/ECDSA-SECP521R1)
//!     Label:
//!     Flags: CKA_WRAP/UNWRAP;
//!     ID: ec:60:3c:df:10:e7:54:7d:ee:27:ee:b9:99:8a:d6:b1:45:c3:73:b8
//! ```
//!
//! Things to note are:
//! - for full key algorithm type (EC/ECDSA-SECP256R1) to be displayed next to private keys
//!     - distinct keys/keypairs need to have unique ids (otherwise it gets confused and can display incorrect keytype)
//!     - for EC keys
//!         - there needs to be both private and public key objects (curve is read from pubkey object)
//!         - pubkey object needs to have the same ID as the privkey object
//!     - for RSA keys
//!         - only private key object itself needs to be present
//!
//! Reference:
//! - thin-edge: docs/src/references/hsm-support.md
//! - PKCS#11: <https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/os/pkcs11-base-v2.40-os.html>

use anyhow::Context;
use asn1_rs::FromDer as _;
use asn1_rs::ToDer;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use cryptoki::context::CInitializeArgs;
use cryptoki::context::Pkcs11;
use cryptoki::error::Error;
use cryptoki::mechanism::Mechanism;
use cryptoki::object::Attribute;
use cryptoki::object::AttributeType;
use cryptoki::object::KeyType;
use cryptoki::object::ObjectClass;
use cryptoki::object::ObjectHandle;
use cryptoki::session::Session;
use cryptoki::session::UserType;
use cryptoki::slot::SlotInfo;
use cryptoki::slot::TokenInfo;
use rsa::pkcs1::EncodeRsaPublicKey;
use rustls::sign::SigningKey;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::error;
use tracing::instrument;
use tracing::trace;
use tracing::warn;

use std::fmt::Debug;
use std::ops::DerefMut;
use std::sync::Arc;
use std::sync::Mutex;

pub use cryptoki::types::AuthPin;

use crate::service;
use crate::service::ChooseSchemeRequest;
use crate::service::ChooseSchemeResponse;
use crate::service::CreateKeyRequest;
use crate::service::CreateKeyResponse;
use crate::service::SecretString;
use crate::service::SignRequestWithSigScheme;
use crate::service::SignResponse;
use crate::service::TedgeP11Service;

mod signing;
pub use signing::Pkcs11Signer;
pub use signing::SigScheme;

mod uri;

/// Parameters used when opening a session.
#[derive(Debug, Clone)]
pub struct SessionParams {
    /// URI identifying the token to which we want to open the session.
    pub(crate) uri: Option<String>,
    /// User PIN value when logging in to the token.
    pub(crate) pin: Option<SecretString>,
}

// oIDs for curves defined here: https://datatracker.ietf.org/doc/html/rfc5480#section-2.1.1.1
// other can be browsed here: https://oid-base.com/get/1.3.132.0.34
const SECP256R1_OID: &str = "1.2.840.10045.3.1.7";
const SECP384R1_OID: &str = "1.3.132.0.34";
const SECP521R1_OID: &str = "1.3.132.0.35";

#[derive(Clone)]
pub struct CryptokiConfigDirect {
    pub module_path: Utf8PathBuf,
    pub pin: SecretString,
    pub uri: Option<Arc<str>>,
}

impl Debug for CryptokiConfigDirect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CryptokiConfigDirect")
            .field("module_path", &self.module_path)
            .field("pin", &"[REDACTED]")
            .field("uri", &self.uri)
            .finish()
    }
}

/// A [`TedgeP11Service`] implementation that uses the loaded cryptoki library to perform PKCS #11 operations.
#[derive(Debug, Clone)]
pub struct Cryptoki {
    context: Arc<Mutex<Pkcs11>>,
    config: CryptokiConfigDirect,
}

impl TedgeP11Service for Cryptoki {
    #[instrument(skip_all)]
    fn choose_scheme(&self, request: ChooseSchemeRequest) -> anyhow::Result<ChooseSchemeResponse> {
        let signing_key = self.signing_key_retry(SessionParams {
            uri: request.uri,
            pin: request.pin,
        })?;
        let offered: Vec<_> = request.offered.into_iter().map(|s| s.0).collect();
        let signer = signing_key
            .choose_scheme(&offered[..])
            .context("failed to choose scheme")?;

        Ok(ChooseSchemeResponse {
            scheme: Some(service::SignatureScheme(signer.scheme())),
            algorithm: service::SignatureAlgorithm(signing_key.algorithm()),
        })
    }

    #[instrument(skip_all)]
    fn sign(&self, request: SignRequestWithSigScheme) -> anyhow::Result<SignResponse> {
        let signing_key = self.signing_key_retry(SessionParams {
            uri: request.uri,
            pin: request.pin,
        })?;
        let signature = signing_key.sign(&request.to_sign, request.sigscheme)?;
        Ok(SignResponse(signature))
    }

    fn get_public_key_pem(&self, uri: Option<&str>) -> anyhow::Result<String> {
        let params = SessionParams {
            uri: uri.map(|s| s.to_string()),
            // PIN is not required when reading public objects like public keys
            pin: None,
        };
        let session = self.open_session_ro(&params)?;
        session.get_public_key_pem()
    }

    fn get_tokens_uris(&self) -> anyhow::Result<Vec<String>> {
        // refresh the slots before getting slot list
        let _ = self.reinit();
        let cryptoki = self.context.lock().unwrap();
        let slots = cryptoki.get_slots_with_initialized_token().unwrap();
        let uris = slots
            .into_iter()
            .filter_map(|slot| {
                cryptoki
                    .get_token_info(slot)
                    .inspect_err(|e| error!(?e, ?slot, "Failed to get_token_info for slot"))
                    .map(|t| export_session_uri(&t))
                    .ok()
            })
            .collect();

        Ok(uris)
    }

    fn create_key(&self, request: CreateKeyRequest) -> anyhow::Result<CreateKeyResponse> {
        let session_params = SessionParams {
            uri: Some(request.uri.to_string()),
            pin: request.pin,
        };
        // NOTE: when writing to HSM, session must always be rw
        let session = self.open_session_rw(&session_params)?;
        let key = session.create_key(request.params)?;
        let pem = session.export_public_key_pem(key)?;
        let uri = session.export_object_uri(key)?;
        Ok(CreateKeyResponse { pem, uri })
    }
}

impl Cryptoki {
    pub fn new(config: CryptokiConfigDirect) -> anyhow::Result<Self> {
        let pkcs11client = Self::load(&config.module_path)?;
        pkcs11client.initialize(CInitializeArgs::OsThreads)?;

        Ok(Self {
            context: Arc::new(Mutex::new(pkcs11client)),
            config,
        })
    }

    /// Reinitializes the PKCS11 library.
    ///
    /// In some libraries, if the slot list changes, this change might not be visible until C_Initialize is called
    /// again ([C_GetSlotList]).
    ///
    /// [C_GetSlotList]: https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/errata01/os/pkcs11-base-v2.40-errata01-os-complete.html#_Toc441755804
    fn reinit(&self) -> anyhow::Result<()> {
        // load a new client before locking so if error we don't poison the mutex
        let new_client = Self::load(&self.config.module_path)?;

        // we never use multiple threads or modify context outside of new and reinit, so we should never panic
        self.context.clear_poison();
        let mut context = self.context.lock().unwrap();
        let old_client = std::mem::replace(context.deref_mut(), new_client);

        // the spec says "(C_Finalize) should be the last Cryptoki call made by an application", so call it on the old
        // client before initializing new client
        // https://docs.oasis-open.org/pkcs11/pkcs11-base/v2.40/errata01/os/pkcs11-base-v2.40-errata01-os-complete.html#_Toc441755803
        old_client.finalize();

        // can return Error::AlreadyInitialized, but it shouldn't, only warn if it does anyway
        if let Err(err) = context.initialize(CInitializeArgs::OsThreads) {
            warn!(?err, "Initializing cryptoki library failed");
        }

        Ok(())
    }

    fn load(module_path: &Utf8Path) -> anyhow::Result<Pkcs11> {
        debug!(%module_path, "Loading PKCS#11 module");
        // can fail with Pkcs11(GeneralError, GetFunctionList) if P11_KIT_SERVER_ADDRESS is wrong
        let client = match Pkcs11::new(module_path) {
            Ok(p) => p,
            // i want to get inner error but i don't know if there is a better way to do this
            Err(Error::LibraryLoading(e)) => {
                return Err(e).context("Failed to load PKCS#11 dynamic object")
            }
            Err(e) => return Err(e).context("Failed to load PKCS#11 dynamic object"),
        };
        Ok(client)
    }

    /// Returns the signing key.
    ///
    /// If the key is not found, we reload the PKCS11 library and retry because some PKCS11
    /// libraries may not always show new slots/objects properly when something changes and we don't
    /// want to restart the server manually. If the key is still missing after a reload, the
    /// original error is returned.
    pub fn signing_key_retry(&self, session_params: SessionParams) -> anyhow::Result<Pkcs11Signer> {
        let signing_key = self
            .open_session_ro(&session_params)
            .and_then(|s| s.signing_key())
            .context("Failed to find a signing key");

        let signing_key = match signing_key {
            Ok(key) => key,
            // refresh the slots only if the key couldn't be found, i.e.:
            // - we didn't find a slot with the token that matches the URI
            // - we didn't find an object on the token that matches the URI
            Err(ref e)
                if format!("{e:#}").contains("Didn't find a slot to use")
                    || format!("{e:#}").contains("Failed to find a key") =>
            {
                warn!("Failed to find a signing key, reloading the library to retry");
                // ensure current session is dropped before opening a new one
                drop(signing_key);
                self.reinit()?;
                self.open_session_ro(&session_params)
                    .and_then(|s| s.signing_key())
                    .context("Failed to find a signing key")?
            }

            Err(e) => return Err(e),
        };

        Ok(signing_key)
    }

    fn open_session_ro<'a>(
        &'a self,
        params: &'a SessionParams,
    ) -> anyhow::Result<CryptokiSession<'a>> {
        self.open_session(params, CryptokiSessionType::ReadOnly)
    }

    fn open_session_rw<'a>(
        &'a self,
        params: &'a SessionParams,
    ) -> anyhow::Result<CryptokiSession<'a>> {
        self.open_session(params, CryptokiSessionType::ReadWrite)
    }

    #[instrument(skip_all)]
    fn open_session<'a>(
        &'a self,
        params: &'a SessionParams,
        session_type: CryptokiSessionType,
    ) -> anyhow::Result<CryptokiSession<'a>> {
        let uri_attributes = self.request_uri(params.uri.as_deref())?;

        let wanted_label = uri_attributes.token.as_ref();
        let wanted_serial = uri_attributes.serial.as_ref();

        let context = match self.context.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };

        debug!(slots = ?get_all_slots_info(&context));
        debug!(tokens = ?get_all_token_info(&context));

        let slots_with_tokens = context.get_slots_with_token()?;
        let tokens: Result<Vec<_>, _> = slots_with_tokens
            .iter()
            .map(|s| {
                context
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

        let slot_info = context.get_slot_info(slot)?;
        let token_info = context.get_token_info(slot)?;
        debug!(?slot_info, ?token_info, "Selected slot");

        let session = match session_type {
            CryptokiSessionType::ReadOnly => context.open_ro_session(slot)?,
            CryptokiSessionType::ReadWrite => context.open_rw_session(slot)?,
        };

        let pin = uri_attributes
            .pin_value
            .as_ref()
            .or(params.pin.as_ref())
            .cloned()
            .as_ref()
            .unwrap_or(&self.config.pin)
            .to_owned();
        let pin = AuthPin::from(pin);

        session.login(UserType::User, Some(&pin))?;
        let session_info = session.get_session_info()?;
        debug!(?session_info, "Opened a readonly session");

        let session = CryptokiSession {
            session,
            token_info,
            uri_attributes,
        };

        let template = [];
        let objects = session.session.find_objects(&template);
        match objects {
            Err(err) => {
                error!(?template, ?err, "failed to find objects");
            }
            Ok(objects) => {
                let objects = objects
                    .into_iter()
                    .flat_map(|o| {
                        let uri = session.export_object_uri(o).inspect_err(
                            |err| error!(?err, object = ?o, "failed to read properties of object"),
                        );
                        uri.map(|u| (o, u)).ok()
                    })
                    .collect::<Vec<_>>();
                trace!(?objects, "Objects found in the token");
            }
        }

        Ok(session)
    }

    fn request_uri<'a>(
        &'a self,
        request_uri: Option<&'a str>,
    ) -> anyhow::Result<uri::Pkcs11Uri<'a>> {
        let mut config_uri = self
            .config
            .uri
            .as_deref()
            .map(|u| uri::Pkcs11Uri::parse(u).context("Failed to parse config PKCS#11 URI"))
            .transpose()?
            .unwrap_or_default();

        let request_uri = request_uri
            .map(|uri| uri::Pkcs11Uri::parse(uri).context("Failed to parse PKCS #11 URI"))
            .transpose()?
            .unwrap_or_default();

        config_uri.append_attributes(request_uri);
        Ok(config_uri)
    }
}

fn get_all_slots_info(cryptoki: &Pkcs11) -> Vec<SlotInfo> {
    let slots = match cryptoki.get_all_slots() {
        Ok(slots) => slots,
        Err(err) => {
            error!(?err, "failed to get slots");
            return vec![];
        }
    };
    slots
        .into_iter()
        .flat_map(|s| {
            cryptoki
                .get_slot_info(s)
                .inspect_err(|err| error!(slot = ?s, ?err, "failed to read slot info from slot"))
                .ok()
        })
        .collect::<Vec<_>>()
}

fn get_all_token_info(cryptoki: &Pkcs11) -> Vec<TokenInfo> {
    let slots = match cryptoki.get_slots_with_token() {
        Ok(slots) => slots,
        Err(err) => {
            error!(?err, "failed to get slots");
            return vec![];
        }
    };

    slots
        .into_iter()
        .flat_map(|s| {
            cryptoki
                .get_token_info(s)
                .inspect_err(|err| error!(slot = ?s, ?err, "failed to read token info from slot"))
                .ok()
        })
        .collect::<Vec<_>>()
}

/// A cryptoki session opened with a token.
struct CryptokiSession<'a> {
    session: Session,
    token_info: TokenInfo,
    uri_attributes: uri::Pkcs11Uri<'a>,
}

#[derive(Debug, Clone, Copy)]
enum CryptokiSessionType {
    ReadOnly,
    ReadWrite,
}

impl CryptokiSession<'_> {
    pub fn signing_key(self) -> anyhow::Result<Pkcs11Signer> {
        // get the signing key
        let key = self.find_key_by_attributes(&self.uri_attributes, ObjectClass::PRIVATE_KEY)?;
        let key_type = self
            .session
            .get_attributes(key, &[AttributeType::KeyType])?
            .into_iter()
            .next()
            .context("no keytype attribute")?;

        let Attribute::KeyType(keytype) = key_type else {
            anyhow::bail!("can't get key type");
        };

        // we need to select a signature scheme to use with a key - each type of key can only have one signature scheme
        // ideally we'd simply get a cryptoki mechanism that corresponds to this sigscheme but it's not possible;
        // instead we have to manually parse additional attributes to select a proper sigscheme; currently don't do it
        // and just select the most common sigscheme for both types of keys

        // NOTE: cryptoki has AttributeType::AllowedMechanisms, but when i use it in get_attributes() with opensc-pkcs11
        // module it gets ignored (not present or supported) and with softhsm2 module it panics(seems to be an issue
        // with cryptoki, but regardless):

        // thread 'main' panicked at library/core/src/panicking.rs:218:5:
        // unsafe precondition(s) violated: slice::from_raw_parts requires the pointer to be aligned and non-null, and the total size of the slice not to exceed `isize::MAX`
        // note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
        // thread caused non-unwinding panic. aborting.
        // Aborted (core dumped)

        let key = match keytype {
            KeyType::EC => {
                let sigscheme =
                    get_ec_mechanism(&self.session, key).unwrap_or(SigScheme::EcdsaNistp256Sha256);

                Pkcs11Signer {
                    session: Arc::new(Mutex::new(self.session)),
                    key,
                    sigscheme,
                    secondary_schemes: Vec::new(),
                }
            }
            KeyType::RSA => Pkcs11Signer {
                session: Arc::new(Mutex::new(self.session)),
                key,
                sigscheme: SigScheme::RsaPssSha256,
                secondary_schemes: vec![SigScheme::RsaPkcs1Sha256],
            },
            _ => anyhow::bail!("unsupported key type"),
        };

        Ok(key)
    }

    fn get_public_key_pem(&self) -> anyhow::Result<String> {
        let key = self.find_key_by_attributes(&self.uri_attributes, ObjectClass::PUBLIC_KEY)?;

        self.export_public_key_pem(key)
    }

    fn find_key_by_attributes(
        &self,
        uri: &uri::Pkcs11Uri,
        class: ObjectClass,
    ) -> anyhow::Result<ObjectHandle> {
        let mut key_template = vec![Attribute::Token(true), Attribute::Class(class)];
        if let Some(object) = &uri.object {
            key_template.push(Attribute::Label(object.as_bytes().to_vec()));
        }
        if let Some(id) = &uri.id {
            key_template.push(Attribute::Id(id.clone()));
        }

        trace!(?key_template, ?uri.object, "Finding a key");

        let mut keys = self
            .session
            .find_objects(&key_template)
            .context("Failed to find private key objects")?
            .into_iter();

        let key = keys.next().context("Failed to find a key")?;
        if keys.len() > 0 {
            warn!(
                "Multiple keys were found. If the wrong one was chosen, please use a URI that uniquely identifies a key."
            )
        }

        Ok(key)
    }

    /// Given a handle to a private or a public key object, export public key in PEM format.
    fn export_public_key_pem(&self, key: ObjectHandle) -> anyhow::Result<String> {
        let keytype = self
            .session
            .get_attributes(key, &[AttributeType::KeyType])?
            .into_iter()
            .next()
            .context("object is not a key")?;
        let Attribute::KeyType(keytype) = keytype else {
            // really all the instances where pkcs11 gives us different attribute than the one we asked for are the same error: invalid behaviour of pkcs11 library or the token
            anyhow::bail!("No keytype");
        };

        let pubkey_der = match keytype {
            KeyType::RSA => {
                let attrs = self.session.get_attributes(
                    key,
                    &[AttributeType::Modulus, AttributeType::PublicExponent],
                )?;
                trace!(?attrs);
                let mut attrs = attrs.into_iter();

                let Attribute::Modulus(modulus) = attrs.next().context("Not modulus")? else {
                    anyhow::bail!("No modulus");
                };
                let modulus = rsa::BigUint::from_bytes_be(&modulus);

                let Attribute::PublicExponent(exponent) = attrs.next().context("Not modulus")?
                else {
                    anyhow::bail!("No public exponent");
                };
                let exponent = rsa::BigUint::from_bytes_be(&exponent);

                let pubkey = rsa::RsaPublicKey::new(modulus, exponent)
                    .context("Failed to construct RSA pubkey from components")?;

                pubkey
                    .to_pkcs1_der()
                    .context("Failed to serialize pubkey as DER")?
                    .into_vec()
            }

            KeyType::EC => {
                let attrs = self
                    .session
                    .get_attributes(key, &[AttributeType::EcPoint])?;
                trace!(?attrs);
                let mut attrs = attrs.into_iter();

                // Elliptic-Curve-Point-to-Octet-String from SEC 1: Elliptic Curve Cryptography (Version 2.0) section 2.3.3 (page 10)
                let ec_point = attrs.next().context("Failed to get pubkey EcPoint")?;
                let Attribute::EcPoint(ec_point) = ec_point else {
                    anyhow::bail!("No ec point");
                };
                let (_, ec_point) =
                    asn1_rs::OctetString::from_der(&ec_point).context("Invalid EcPoint")?;
                ec_point.into_cow().to_vec()
            }
            _ => anyhow::bail!("unsupported keytype"),
        };
        let pubkey_pem = pem::Pem::new("PUBLIC KEY", pubkey_der);
        let pubkey_pem = pem::encode(&pubkey_pem);

        Ok(pubkey_pem)
    }

    fn export_object_uri(&self, object: ObjectHandle) -> anyhow::Result<String> {
        let template = &[AttributeType::Id, AttributeType::Label];
        let attrs = self.session.get_attributes(object, template)?.into_iter();

        let mut key_uri = export_session_uri(&self.token_info);

        for attr in attrs {
            match attr {
                Attribute::Id(id) => {
                    key_uri.push(';');
                    key_uri.push_str("id=");
                    // from RFC section 2.3: Note that the value of the "id" attribute SHOULD NOT be encoded as UTF-8 because it can
                    // contain non-textual data, instead it SHOULD be entirely percent-encoded
                    for byte in &id {
                        key_uri.push_str(percent_encoding::percent_encode_byte(*byte));
                    }
                }
                Attribute::Label(label) => {
                    let label = std::str::from_utf8(&label).context("label should be utf-8")?;
                    key_uri.push(';');
                    key_uri.push_str("object=");
                    let label = uri::percent_encode(label);
                    key_uri.push_str(&label);
                }
                other => warn!(asked = ?template, got= ?other, "Got invalid attribute"),
            }
        }

        // omit the "type" attribute since its not relevant when used as device.key_uri, which is intended use for this produced value

        anyhow::ensure!(key_uri.starts_with("pkcs11:"));

        Ok(key_uri)
    }

    /// Create a new keypair on the token.
    fn create_key(&self, params: CreateKeyParams) -> anyhow::Result<ObjectHandle> {
        let (mechanism, attrs_pub, attrs_priv) = match params.key {
            KeyTypeParams::Rsa { bits } => {
                anyhow::ensure!(
                    bits == 2048 || bits == 3072 || bits == 4096,
                    "Invalid bits value: only 2048/3072/4096 key sizes are valid"
                );
                (
                    Mechanism::RsaPkcsKeyPairGen,
                    vec![Attribute::ModulusBits(
                        // u64 or u32 depending on the platform
                        std::os::raw::c_ulong::from(bits).into(),
                    )],
                    vec![],
                )
            }
            KeyTypeParams::Ec { curve } => {
                // serialize chosen curve to CKA_EC_PARAMS choice structure
                // https://docs.oasis-open.org/pkcs11/pkcs11-curr/v3.0/os/pkcs11-curr-v3.0-os.html#_Toc30061181
                let oid = match curve {
                    256 => SECP256R1_OID,
                    384 => SECP384R1_OID,
                    _ => anyhow::bail!("Invalid EC curve value: only P256/P384 valid"),
                };
                let components: Vec<u64> = oid.split('.').map(|c| c.parse().unwrap()).collect();
                let curve_oid = asn1_rs::Oid::from(&components)
                    .unwrap()
                    .to_der_vec()
                    .unwrap();
                trace!("{curve_oid:x?}");
                (
                    Mechanism::EccKeyPairGen,
                    vec![Attribute::EcParams(curve_oid)],
                    vec![],
                )
            }
        };

        let id = params.id.unwrap_or(rand::random::<[u8; 20]>().to_vec());

        let objects = self.session.find_objects(&[Attribute::Id(id.clone())])?;
        anyhow::ensure!(
            objects.is_empty(),
            "Object with this id already exists on the token"
        );

        let mut pub_key_template = attrs_pub;
        pub_key_template.extend_from_slice(&[
            Attribute::Token(true),
            Attribute::Private(false),
            Attribute::Verify(true),
            Attribute::Encrypt(true),
            Attribute::Label(params.label.clone().into()),
            Attribute::Id(id.to_vec()),
        ]);

        let mut priv_key_template = attrs_priv;
        priv_key_template.extend_from_slice(&[
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sensitive(true),
            Attribute::Extractable(false),
            Attribute::Sign(true),
            Attribute::Decrypt(true),
            Attribute::Label(params.label.clone().into()),
            Attribute::Id(id.to_vec()),
        ]);

        trace!(?pub_key_template, ?priv_key_template, "Generating keypair");
        let (pub_handle, _priv_handle) = self
            .session
            .generate_key_pair(&mechanism, &pub_key_template, &priv_key_template)
            .context("Failed to generate keypair")?;

        Ok(pub_handle)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateKeyParams {
    pub key: KeyTypeParams,
    pub label: String,
    pub id: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyTypeParams {
    Rsa { bits: u16 },
    Ec { curve: u16 },
}

fn get_ec_mechanism(session: &Session, key: ObjectHandle) -> anyhow::Result<SigScheme> {
    let key_params = &[AttributeType::EcParams];
    let attrs = session
        .get_attributes(key, key_params)
        .context("Failed to get key params")?;
    trace!(?attrs);

    let attr = attrs
        .into_iter()
        .next()
        .context("Failed to get EcParams attribute")?;
    let Attribute::EcParams(ecparams) = attr else {
        anyhow::bail!("Failed to get EcParams attribute");
    };

    // this can be oid, but also a bunch of other things
    // https://docs.oasis-open.org/pkcs11/pkcs11-curr/v3.0/os/pkcs11-curr-v3.0-os.html#_Toc30061181
    let (_, ecparams) = asn1_rs::Any::from_der(&ecparams).context("Failed to parse EC_PARAMS")?;
    let oid = ecparams.as_oid().context("EC_PARAMS isn't an oID")?;
    let oid = oid.to_id_string();
    match oid.as_str() {
        SECP256R1_OID => Ok(SigScheme::EcdsaNistp256Sha256),
        SECP384R1_OID => Ok(SigScheme::EcdsaNistp384Sha384),
        SECP521R1_OID => Ok(SigScheme::EcdsaNistp521Sha512),
        _ => anyhow::bail!("Parsed oID({oid}) doesn't match any supported EC curve"),
    }
}

/// Generates PKCS11 URI of the selected token.
///
/// The generated URI attempts to be similar to URIs generated by gnutls. Notably, "slot-description", "slot-id", and
/// "slot-manufacturer" attributes are missing from gnutls URIs, perhaps for portability (URI points to the same thing
/// even if token is reinserted into a different slot).
fn export_session_uri(token_info: &TokenInfo) -> String {
    let mut uri = String::from("pkcs11:");

    uri.push_str("model=");
    let model = uri::percent_encode(token_info.model());
    uri.push_str(&model);

    uri.push(';');
    uri.push_str("manufacturer=");
    let manufacturer = uri::percent_encode(token_info.manufacturer_id());
    uri.push_str(&manufacturer);

    uri.push(';');
    uri.push_str("serial=");
    let serial = uri::percent_encode(token_info.serial_number());
    uri.push_str(&serial);

    uri.push(';');
    uri.push_str("token=");
    let token = uri::percent_encode(token_info.label());
    uri.push_str(&token);

    uri
}
