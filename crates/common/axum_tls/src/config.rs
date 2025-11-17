use crate::load_cert;
use crate::load_pkey;
use crate::read_trust_store;
use crate::ssl_config;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::PrivateKeyDer;
use rustls::RootCertStore;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::io::Cursor;
use std::path::Path;
use tedge_config::OptionalConfig;
use tracing::info;
use yansi::Paint;

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
///     config.http.ca_path.as_ref(),
///     "File transfer service",
/// )?;
/// # Ok(())
/// # }
/// ```
///
/// In a test, we can instead use [InjectedValue]
///
/// ```
/// # fn main() -> anyhow::Result<()> {
/// use rustls::RootCertStore;
/// use axum_tls::config::{InjectedValue, load_ssl_config};
/// use tedge_config::{OptionalConfig, TEdgeConfig};
///
/// let cert = rcgen::generate_simple_self_signed(["localhost".to_owned()]).unwrap();
/// let cert_pem = cert.cert.pem();
/// let key_pem = cert.signing_key.serialize_pem();
///
/// let config = load_ssl_config(
///     OptionalConfig::present(InjectedValue(cert_pem), "http.cert_path"),
///     OptionalConfig::present(InjectedValue(key_pem), "http.key_path"),
///     OptionalConfig::<InjectedValue<RootCertStore>>::empty("http.ca_path"),
///     "File transfer service",
/// )?;
/// # Ok(())
/// # }
/// ```
///
pub fn load_ssl_config(
    cert_path: OptionalConfig<impl PemReader>,
    key_path: OptionalConfig<impl PemReader>,
    ca_path: OptionalConfig<impl TrustStoreLoader>,
    service_name: &'static str,
) -> anyhow::Result<Option<rustls::ServerConfig>> {
    let enabled = Paint::green("enabled").bold();
    let disabled = Paint::red("disabled").bold();
    let service_name = service_name.bold();
    let cert_key = cert_path.key();
    let key_key = key_path.key();
    let ca_key = ca_path.key();
    if let Some((cert, key)) = load_certificate_and_key(&cert_path, &key_path)? {
        let trust_store = match ca_path.or_none() {
            Some(path) => path
                .load_trust_store()
                .map(Some)
                .with_context(|| format!("reading root certificates configured in `{ca_key}`",))?,
            None => None,
        };
        let ca_state = if let Some(store) = &trust_store {
            let count = store.len();
            format!("{enabled} ({count} certificates found)")
        } else {
            format!("{disabled}")
        };

        info!(target: "HTTP Server", "{service_name} has HTTPS {enabled} (configured in `{cert_key}`/`{key_key}`) and certificate authentication {ca_state} (configured in `{ca_key}`)", );
        Ok(Some(ssl_config(cert, key, trust_store)?))
    } else {
        info!(target: "HTTP Server", "{service_name} has HTTPS {disabled} (configured in `{cert_key}`/`{key_key}`) and certificate authentication {disabled} (configured in `{ca_key}`)");
        Ok(None)
    }
}

type CertKeyPair = (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>);

fn load_certificate_and_key(
    cert_path: &OptionalConfig<impl PemReader>,
    key_path: &OptionalConfig<impl PemReader>,
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
    fn load_trust_store(&self) -> anyhow::Result<RootCertStore>;
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
/// let pem_data = cert.cert.pem();
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
    type Read<'a>
        = File
    where
        Self: 'a;
    fn open(&self) -> io::Result<File> {
        File::open(self)
    }
}

impl<P: AsRef<Utf8Path> + 'static> TrustStoreLoader for P {
    fn load_trust_store(&self) -> anyhow::Result<RootCertStore> {
        read_trust_store(self.as_ref())
    }
}

impl TrustStoreLoader for InjectedValue<RootCertStore> {
    fn load_trust_store(&self) -> anyhow::Result<RootCertStore> {
        Ok(self.0.clone())
    }
}
