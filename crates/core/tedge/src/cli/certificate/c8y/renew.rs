use crate::cli::certificate::c8y::create_device_csr;
use crate::cli::certificate::c8y::read_csr_from_file;
use crate::cli::certificate::c8y::store_device_cert;
use crate::command::Command;
use crate::error;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use anyhow::Error;
use c8y_api::http_proxy::C8yEndPoint;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use hyper::header::CONTENT_TYPE;
use hyper::StatusCode;
use reqwest::Identity;
use reqwest::Response;
use url::Url;

/// Command to renew a device certificate from Cumulocity
pub struct RenewCertCmd {
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,

    /// Cumulocity endpoint from where the device got his current certificate
    pub c8y: C8yEndPoint,

    /// Root certificates used to authenticate the Cumulocity instance
    pub root_certs: CloudRootCerts,

    /// TLS Client configuration
    pub identity: Option<Identity>,

    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,

    /// The path where the device private key will be stored
    pub key_path: Utf8PathBuf,

    /// The path where the device CSR file will be stored
    pub csr_path: Utf8PathBuf,

    /// Tell if the CSR has to be generated or is ready to be used
    pub generate_csr: bool,
}

#[async_trait::async_trait]
impl Command for RenewCertCmd {
    fn description(&self) -> String {
        format!("Renew the device certificate from {}", self.c8y_url())
    }

    async fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        Ok(self.renew_device_certificate().await?)
    }
}

impl RenewCertCmd {
    /// Cumulocity instance from where the device got his current certificate
    fn c8y_url(&self) -> String {
        self.c8y.get_base_url()
    }

    async fn renew_device_certificate(&self) -> Result<(), Error> {
        if self.generate_csr {
            create_device_csr(
                self.device_id.clone(),
                self.key_path.clone(),
                self.csr_path.clone(),
            )
            .await?;
        }
        let csr = read_csr_from_file(&self.csr_path).await?;

        let http_builder = self.root_certs.client_builder();
        let http_builder = if let Some(identity) = &self.identity {
            http_builder.identity(identity.clone())
        } else {
            http_builder
        };
        let http = http_builder.build()?;

        let url = format!(
            "{}/.well-known/est/simplereenroll",
            self.c8y.get_proxy_url()
        );
        let url = Url::parse(&url)?;
        let result = self.post_device_csr(&http, &url, &csr).await;
        match result {
            Ok(response) if response.status() == StatusCode::OK => {
                if let Ok(cert) = response.text().await {
                    store_device_cert(&self.cert_path, cert).await?;
                    return Ok(());
                }
                error!("Fail to extract a certificate from the response returned by {url}");
            }
            Ok(response) => {
                error!(
                    "The device certificate cannot be renewed from {url}:\n\t{} {}",
                    response.status(),
                    response.text().await.unwrap_or("".to_string())
                );
            }
            Err(err) => {
                error!(
                    "Fail to connect to {url}: {:?}",
                    get_webpki_error_from_reqwest(err)
                )
            }
        }

        Ok(())
    }

    /// Post the device CSR
    async fn post_device_csr(
        &self,
        http: &reqwest::Client,
        url: &Url,
        csr: &str,
    ) -> Result<Response, reqwest::Error> {
        http.post(url.clone())
            .header(CONTENT_TYPE, "application/pkcs10")
            .body(csr.to_string())
            .send()
            .await
    }
}
