use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use reqwest::Certificate;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::ReadDirStream;
use tokio_stream::Stream;
use tokio_stream::StreamExt as _;

#[derive(Debug, Clone)]
pub struct CloudHttpConfig {
    certificates: Arc<[Certificate]>,
    proxy: Option<reqwest::Proxy>,
}

impl CloudHttpConfig {
    pub fn new(certificates: impl Into<Arc<[Certificate]>>, proxy: Option<reqwest::Proxy>) -> Self {
        Self {
            certificates: certificates.into(),
            proxy,
        }
    }

    pub fn test_value() -> Self {
        Self {
            certificates: Arc::new([]),
            proxy: None,
        }
    }

    #[allow(clippy::disallowed_types)]
    pub fn client_builder(&self) -> reqwest::ClientBuilder {
        let builder = self
            .certificates
            .iter()
            .cloned()
            .fold(reqwest::ClientBuilder::new(), |builder, cert| {
                builder.add_root_certificate(cert)
            });

        if let Some(proxy) = self.proxy.clone() {
            builder.proxy(proxy)
        } else {
            builder.no_proxy()
        }
    }

    #[allow(clippy::disallowed_types)]
    pub fn client(&self) -> reqwest::Client {
        self.client_builder()
            .build()
            .expect("Valid reqwest client builder configuration")
    }
}

/// Read a directory into a [RootCertStore]
pub async fn read_trust_store(ca_dir_or_file: &Utf8Path) -> anyhow::Result<Vec<Certificate>> {
    let mut certs = Vec::new();
    let mut stream = stream_file_or_directory(ca_dir_or_file).await;
    while let Some(path) = stream.next().await {
        let path =
            path.with_context(|| format!("reading metadata for file at {ca_dir_or_file}"))?;

        if path.is_dir() {
            continue;
        }

        let pem_bytes = match tokio::fs::read(&path).await {
            Ok(pem_file) => pem_file,
            err if path == ca_dir_or_file => {
                err.with_context(|| format!("failed to read from path {path:?}"))?
            }
            Err(_other_unreadable_file) => continue,
        };

        let ders = CertificateDer::pem_slice_iter(&pem_bytes)
            .map(|res| Ok(Certificate::from_der(&res?)?))
            .collect::<anyhow::Result<Vec<_>>>()
            .with_context(|| format!("reading {path}"))?;
        certs.extend(ders)
    }

    Ok(certs)
}

async fn stream_file_or_directory(
    possible_dir: &Utf8Path,
) -> Pin<Box<dyn Stream<Item = anyhow::Result<Utf8PathBuf>> + Send + 'static>> {
    let base_path = possible_dir.to_path_buf();
    if let Ok(dir) = tokio::fs::read_dir(possible_dir).await {
        let dir_stream = ReadDirStream::new(dir);
        Box::pin(dir_stream.map(move |file| match file {
            Ok(file) => {
                let mut path = base_path.clone();
                path.push(file.file_name().to_str().ok_or(anyhow::anyhow!(
                    "encountered non-utf8 path: {}",
                    file.file_name().to_string_lossy()
                ))?);
                Ok(path)
            }
            Err(e) => Err(e).with_context(|| format!("reading metadata for file in {base_path}")),
        }))
    } else {
        Box::pin(tokio_stream::once(Ok(base_path)))
    }
}
