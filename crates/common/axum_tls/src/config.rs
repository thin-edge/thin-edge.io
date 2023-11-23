use crate::load_cert;
use crate::load_pkey;
use crate::read_trust_store;
use crate::ssl_config;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use rustls::RootCertStore;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::path::Path;
use tedge_config::OptionalConfig;

/// Loads the relevant [rustls::ServerConfig] from configured values for `cert_path`, `key_path` and `ca_path`
///
/// In production use, all the paths should be passed in as [OptionalConfig]s from [TEdgeConfig]
///
/// ```no_run
/// # fn main() -> anyhow::Result<()> {
/// use axum_tls::config::load_ssl_config;
/// use tedge_config::TEdgeConfig;
///
/// let config: TEdgeConfig = unimplemented!("read config");
///
/// let config = load_ssl_config(
///     config.http.cert_path.as_ref(),
///     config.http.key_path.as_ref(),
///     config.http.ca_path.as_ref()
/// )?;
/// # Ok(())
/// # }
/// ```
///
/// In a test, we can instead use [InjectedValue]
///
/// ```
/// # fn main() -> anyhow::Result<()> {
/// use axum_tls::config::{InjectedValue, load_ssl_config};
/// use tedge_config::{OptionalConfig, TEdgeConfig};
///
/// let cert = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
/// let cert_pem = cert.serialize_pem().unwrap();
/// let key_pem = cert.serialize_private_key_pem();
///
/// let config = load_ssl_config(
///     OptionalConfig::present(InjectedValue(cert_pem), "http.cert_path"),
///     OptionalConfig::present(InjectedValue(key_pem), "http.key_path"),
///     InjectedValue(None), // ...or `Some(RootCertStore)`
/// )?;
/// # Ok(())
/// # }
/// ```
///
pub fn load_ssl_config(
    cert_path: OptionalConfig<impl PemReader>,
    key_path: OptionalConfig<impl PemReader>,
    ca_path: impl TrustStoreLoader,
) -> anyhow::Result<Option<rustls::ServerConfig>> {
    if let Some((cert, key)) = load_certificate_and_key(cert_path, key_path)? {
        Ok(Some(ssl_config(cert, key, ca_path.load_trust_store()?)?))
    } else {
        Ok(None)
    }
}

type CertKeyPair = (Vec<Vec<u8>>, Vec<u8>);

fn load_certificate_and_key(
    cert_path: OptionalConfig<impl PemReader>,
    key_path: OptionalConfig<impl PemReader>,
) -> anyhow::Result<Option<CertKeyPair>> {
    let paths = tedge_config::all_or_nothing((cert_path.as_ref(), key_path.as_ref()))
        .map_err(|e| anyhow!("{e}"))?;

    if let Some((cert_file, key_file)) = paths {
        Ok(Some((
            load_cert(cert_file).with_context(|| {
                format!("reading certificate configured in `{}`", cert_path.key())
            })?,
            load_pkey(key_file).with_context(|| {
                format!("reading private key configured in `{}`", key_path.key())
            })?,
        )))
    } else {
        Ok(None)
    }
}

pub trait PemReader: Debug {
    type Read<'a>: io::Read
    where
        Self: 'a;

    fn open(&self) -> io::Result<Self::Read<'_>>;
}

pub trait TrustStoreLoader {
    fn load_trust_store(&self) -> anyhow::Result<Option<RootCertStore>>;
}

#[derive(Debug)]
/// An injected value, used to avoid reading from the file system in unit tests
///
/// For example, a certificate path can be replaced with an [InjectedValue<String>]
/// where the [String] inside is a PEM-encoded certificate
///
/// ```
/// use axum_tls::config::InjectedValue;
/// use axum_tls::load_cert;
/// let cert = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
/// let pem_data = cert.serialize_pem().unwrap();
///
/// let loaded_chain = load_cert(&InjectedValue(pem_data)).unwrap();
///
/// assert_eq!(loaded_chain.len(), 1);
/// ```
pub struct InjectedValue<S>(pub S);

impl PemReader for InjectedValue<String> {
    type Read<'a> = Cursor<&'a [u8]>;
    fn open(&self) -> io::Result<Self::Read<'_>> {
        Ok(Cursor::new(self.0.as_bytes()))
    }
}

impl<P: AsRef<Path> + Debug + ?Sized> PemReader for P {
    type Read<'a> = File where Self: 'a;
    fn open(&self) -> io::Result<File> {
        File::open(self)
    }
}

impl<P: AsRef<Utf8Path> + 'static> TrustStoreLoader for OptionalConfig<P> {
    fn load_trust_store(&self) -> anyhow::Result<Option<RootCertStore>> {
        match self.or_none() {
            Some(s) => read_trust_store(s.as_ref()).map(Some).with_context(|| {
                format!("reading root certificates configured in `{}`", self.key())
            }),
            None => Ok(None),
        }
    }
}

impl TrustStoreLoader for InjectedValue<Option<RootCertStore>> {
    fn load_trust_store(&self) -> anyhow::Result<Option<RootCertStore>> {
        Ok(self.0.clone())
    }
}
