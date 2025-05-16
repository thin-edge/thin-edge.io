use crate::certificate_cn;
use crate::cli::certificate::c8y::create_device_csr;
use crate::cli::certificate::c8y::read_csr_from_file;
use crate::cli::certificate::c8y::store_device_cert;
use crate::cli::certificate::show::ShowCertCmd;
use crate::command::Command;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Error;
use c8y_api::http_proxy::C8yEndPoint;
use camino::Utf8PathBuf;
use certificate::CloudHttpConfig;
use certificate::CsrTemplate;
use hyper::header::CONTENT_TYPE;
use hyper::StatusCode;
use reqwest::Identity;
use reqwest::Response;
use tedge_config::TEdgeConfig;
use url::Url;

/// Command to renew a device certificate from Cumulocity
pub struct RenewCertCmd {
    /// Cumulocity endpoint from where the device got his current certificate
    pub c8y: C8yEndPoint,

    /// Root certificates used to authenticate the Cumulocity instance
    pub http_config: CloudHttpConfig,

    /// TLS Client configuration
    pub identity: Option<Identity>,

    /// The path of the certificate to be renewed
    pub cert_path: Utf8PathBuf,

    /// The path where the new certificate will be stored
    pub new_cert_path: Utf8PathBuf,

    /// The path of the private key to re-use
    pub key_path: Utf8PathBuf,

    /// The path where the device CSR file will be stored
    pub csr_path: Utf8PathBuf,

    /// Tell if the CSR has to be generated or is ready to be used
    pub generate_csr: bool,

    /// CSR template
    pub csr_template: CsrTemplate,
}

#[async_trait::async_trait]
impl Command for RenewCertCmd {
    fn description(&self) -> String {
        format!(
            "renew the device certificate via Cumulocity HTTP proxy {}",
            self.c8y_url()
        )
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        self.renew_device_certificate().await?;
        eprintln!("Certificate renewed successfully");
        eprintln!("    For an un-interrupted service:");
        eprintln!("    => the device has to be reconnected to the cloud\n");
        ShowCertCmd::show(&self.new_cert_path).await?;
        Ok(())
    }
}

impl RenewCertCmd {
    /// Cumulocity instance from where the device got his current certificate
    fn c8y_url(&self) -> String {
        self.c8y.get_base_url()
    }

    async fn renew_device_certificate(&self) -> Result<(), Error> {
        if self.generate_csr {
            let common_name = certificate_cn(&self.cert_path).await?;
            create_device_csr(
                common_name,
                self.key_path.clone(),
                self.csr_path.clone(),
                self.csr_template.clone(),
            )
            .await?;
        }
        let csr = read_csr_from_file(&self.csr_path).await?;

        let http_builder = self.http_config.client_builder();
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
            Ok(response) if response.status() == StatusCode::OK => match response.text().await {
                Ok(cert) => {
                    store_device_cert(&self.new_cert_path, cert).await?;
                    Ok(())
                }
                Err(err) => Err(anyhow!(
                    "Fail to extract a certificate from the response: {err}"
                )),
            },
            Ok(response) => Err(anyhow!(
                "The request failed with {}:\n\t{}",
                response.status(),
                response.text().await.unwrap_or("".to_string())
            )),
            Err(err) => Err(Error::new(get_webpki_error_from_reqwest(err))
                .context(format!("Fail to connect to Cumulocity HTTP proxy {url}"))),
        }
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
