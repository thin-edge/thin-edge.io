use crate::cli::certificate::c8y::create_device_csr;
use crate::cli::certificate::c8y::read_csr_from_file;
use crate::cli::certificate::c8y::store_device_cert;
use crate::command::Command;
use crate::error;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use crate::warning;
use anyhow::Context;
use anyhow::Error;
use c8y_api::json_c8y_deserializer::C8yAPIError;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use certificate::CsrTemplate;
use hyper::StatusCode;
use reqwest::header::CONTENT_TYPE;
use reqwest::Response;
use std::io::Write;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use url::Url;

/// Command to request and download a device certificate from Cumulocity
pub struct DownloadCertCmd {
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,

    /// The security token assigned to this device when registered to Cumulocity
    pub one_time_password: String,

    /// Cumulocity instance where the device has been registered
    pub c8y_url: HostPort<HTTPS_PORT>,

    /// Root certificates used to authenticate the Cumulocity instance
    pub root_certs: CloudRootCerts,

    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,

    /// The path where the device private key will be stored
    pub key_path: Utf8PathBuf,

    /// The path where the device CSR file will be stored
    pub csr_path: Utf8PathBuf,

    /// Tell if the CSR has to be generated or is ready to be used
    pub generate_csr: bool,

    /// Delay between two attempts, polling till the device is registered
    pub retry_every: Duration,

    /// Maximum time waiting for the device to be registered
    pub max_timeout: Duration,

    /// CSR template
    pub csr_template: CsrTemplate,
}

#[async_trait::async_trait]
impl Command for DownloadCertCmd {
    fn description(&self) -> String {
        format!(
            "Download a certificate from {} for the device {}",
            self.c8y_url, self.device_id
        )
    }

    async fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        Ok(self.download_device_certificate().await?)
    }
}

impl DownloadCertCmd {
    async fn download_device_certificate(&self) -> Result<(), Error> {
        let (common_name, one_time_password) = self.get_registration_data().await?;
        if self.generate_csr {
            create_device_csr(
                common_name.clone(),
                self.key_path.clone(),
                self.csr_path.clone(),
                self.csr_template.clone(),
            )
            .await
            .with_context(|| format!("Fail to create the device CSR {}", self.csr_path))?;
        }
        let csr = read_csr_from_file(&self.csr_path).await?;

        let http = self.root_certs.client();
        let c8y_url = &self.c8y_url;
        let url = format!("https://{c8y_url}/.well-known/est/simpleenroll");
        let url = Url::parse(&url)?;

        let started = std::time::Instant::now();
        loop {
            let result = self
                .post_device_csr(&http, &url, &common_name, &one_time_password, &csr)
                .await;
            match result {
                Ok(response) if response.status() == StatusCode::OK => {
                    if let Ok(cert) = response.text().await {
                        store_device_cert(&self.cert_path, cert).await?;
                        return Ok(());
                    }
                    error!("Fail to extract a certificate from the response returned by {c8y_url}");
                }
                Ok(response) => {
                    let error = Self::c8y_error_message(response).await;
                    error!("The device {common_name} is not registered yet on {c8y_url}: {error}");
                }
                Err(err) => {
                    error!(
                        "Fail to connect to {}: {:?}",
                        self.c8y_url,
                        get_webpki_error_from_reqwest(err)
                    )
                }
            }
            if started.elapsed() > self.max_timeout {
                return Err(anyhow::anyhow!(
                    "Maximum timeout elapsed. No certificate has been downloaded"
                ));
            }
            warning!("Will retry in {} seconds", self.retry_every.as_secs());
            tokio::time::sleep(self.retry_every).await;
        }
    }

    /// Prompt the user for the device id and the security token
    ///
    /// - unless already set on the command line or using env variables.
    async fn get_registration_data(&self) -> Result<(String, String), std::io::Error> {
        let self_device_id = self.device_id.clone();
        let self_one_time_password = self.one_time_password.clone();
        tokio::task::spawn_blocking(move || {
            let device_id = if self_device_id.is_empty() {
                print!("Enter device id: ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                input.trim_end_matches(['\n', '\r']).to_string()
            } else {
                self_device_id
            };

            // Read the security token from /dev/tty
            let one_time_password = if self_one_time_password.is_empty() {
                rpassword::read_password_from_tty(Some("Enter one-time password: "))?
            } else {
                self_one_time_password
            };

            Ok((device_id, one_time_password))
        })
        .await?
    }

    /// Post the device CSR
    async fn post_device_csr(
        &self,
        http: &reqwest::Client,
        url: &Url,
        username: &str,
        password: &str,
        csr: &str,
    ) -> Result<Response, reqwest::Error> {
        http.post(url.clone())
            .basic_auth(username, Some(password))
            .header(CONTENT_TYPE, "application/pkcs10")
            .body(csr.to_string())
            .send()
            .await
    }

    async fn c8y_error_message(response: Response) -> String {
        let status = response.status().to_string();
        if let Ok(C8yAPIError { message, .. }) = response.json().await {
            format!("{status}: {}", message)
        } else {
            status
        }
    }
}
