use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use reqwest::Certificate;
use std::fs::File;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CloudRootCerts {
    certificates: Arc<[Certificate]>,
}

impl CloudRootCerts {
    #[allow(clippy::disallowed_types)]
    pub fn client_builder(&self) -> reqwest::ClientBuilder {
        self.certificates
            .iter()
            .cloned()
            .fold(reqwest::ClientBuilder::new(), |builder, cert| {
                builder.add_root_certificate(cert)
            })
    }

    #[allow(clippy::disallowed_types)]
    pub fn client(&self) -> reqwest::Client {
        self.client_builder()
            .build()
            .expect("Valid reqwest client builder configuration")
    }

    #[allow(clippy::disallowed_types)]
    #[cfg(feature = "reqwest-blocking")]
    pub fn blocking_client_builder(&self) -> reqwest::blocking::ClientBuilder {
        self.certificates
            .iter()
            .cloned()
            .fold(reqwest::blocking::ClientBuilder::new(), |builder, cert| {
                builder.add_root_certificate(cert)
            })
    }

    #[allow(clippy::disallowed_types)]
    #[cfg(feature = "reqwest-blocking")]
    pub fn blocking_client(&self) -> reqwest::blocking::Client {
        self.blocking_client_builder()
            .build()
            .expect("Valid reqwest client builder configuration")
    }
}

impl From<Arc<[Certificate]>> for CloudRootCerts {
    fn from(certificates: Arc<[Certificate]>) -> Self {
        Self { certificates }
    }
}

impl From<[Certificate; 0]> for CloudRootCerts {
    fn from(certificates: [Certificate; 0]) -> Self {
        Self {
            certificates: Arc::new(certificates),
        }
    }
}

/// Read a directory into a [RootCertStore]
pub fn read_trust_store(ca_dir_or_file: &Utf8Path) -> anyhow::Result<Vec<Certificate>> {
    let mut certs = Vec::new();
    for path in iter_file_or_directory(ca_dir_or_file) {
        let path =
            path.with_context(|| format!("reading metadata for file at {ca_dir_or_file}"))?;

        if path.is_dir() {
            continue;
        }

        let mut pem_file = match File::open(&path).map(std::io::BufReader::new) {
            Ok(pem_file) => pem_file,
            err if path == ca_dir_or_file => {
                err.with_context(|| format!("failed to read from path {path:?}"))?
            }
            Err(_other_unreadable_file) => continue,
        };

        let ders = rustls_pemfile::certs(&mut pem_file)
            .with_context(|| format!("reading {path}"))?
            .into_iter()
            .map(|der| Certificate::from_der(&der).unwrap());
        certs.extend(ders)
    }

    Ok(certs)
}

fn iter_file_or_directory(
    possible_dir: &Utf8Path,
) -> Box<dyn Iterator<Item = anyhow::Result<Utf8PathBuf>> + 'static> {
    let path = possible_dir.to_path_buf();
    if let Ok(dir) = possible_dir.read_dir_utf8() {
        Box::new(dir.map(move |file| match file {
            Ok(file) => {
                let mut path = path.clone();
                path.push(file.file_name());
                Ok(path)
            }
            Err(e) => Err(e).with_context(|| format!("reading metadata for file in {path}")),
        }))
    } else {
        Box::new([Ok(path)].into_iter())
    }
}
