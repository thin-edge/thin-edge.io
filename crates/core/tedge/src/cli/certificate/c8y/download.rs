use crate::cli::certificate::create_csr::CreateCsrCmd;
use crate::command::Command;
use crate::error;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use crate::read_cert_to_string;
use crate::warning;
use crate::CertError;
use anyhow::Context;
use anyhow::Error;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use certificate::NewCertificateConfig;
use hyper::StatusCode;
use reqwest::header::CONTENT_TYPE;
use reqwest::Response;
use std::fs::Permissions;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use url::Url;

/// Command to request and download a device certificate from Cumulocity
pub struct DownloadCertCmd {
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,

    /// The security token assigned to this device when registered to Cumulocity
    pub security_token: String,

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
        let (common_name, security_token) = self.get_registration_data()?;
        let csr = self
            .create_device_csr(common_name.clone())
            .await
            .with_context(|| format!("Fail to create the device CSR {}", self.csr_path))?;

        let http = self.root_certs.client();
        let url = format!("https://{}/.well-known/est/simpleenroll", self.c8y_url);
        let url = Url::parse(&url)?;

        loop {
            let result = self
                .post_device_csr(&http, &url, &common_name, &security_token, &csr)
                .await;
            match result {
                Ok(response) if response.status() == StatusCode::OK => {
                    if let Ok(cert) = response.text().await {
                        self.store_device_cert(cert).await?;
                        return Ok(());
                    }
                    error!(
                        "Fail to extract a certificate from the response returned by {}",
                        self.c8y_url
                    );
                }
                Ok(response) => {
                    error!(
                        "The device {} is not registered yet on {}: {}:{:?}",
                        common_name,
                        self.c8y_url,
                        response.status(),
                        response.text().await
                    );
                }
                Err(err) => {
                    error!(
                        "Fail to connect to {}: {:?}",
                        self.c8y_url,
                        get_webpki_error_from_reqwest(err)
                    )
                }
            }
            warning!("Will retry in 5 seconds");
            std::thread::sleep(Duration::from_secs(5));
        }
    }

    /// Prompt the user for the device id and the security token
    ///
    /// - unless already set on the command line or using env variables.
    fn get_registration_data(&self) -> Result<(String, String), std::io::Error> {
        let device_id = if self.device_id.is_empty() {
            print!("Enter device id: ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            input.trim_end_matches(['\n', '\r']).to_string()
        } else {
            self.device_id.clone()
        };

        // Read the security token from /dev/tty
        let security_token = if self.security_token.is_empty() {
            rpassword::read_password_from_tty(Some("Enter security token: "))?
        } else {
            self.security_token.clone()
        };

        Ok((device_id, security_token))
    }

    /// Create the device private key and CSR
    async fn create_device_csr(&self, common_name: String) -> Result<String, CertError> {
        let config = NewCertificateConfig::default();
        let create_cmd = CreateCsrCmd {
            id: common_name,
            csr_path: self.csr_path.clone(),
            key_path: self.key_path.clone(),
            user: "tedge".to_string(),
            group: "tedge".to_string(),
        };
        create_cmd
            .create_certificate_signing_request(&config)
            .await?;
        read_cert_to_string(&self.csr_path)
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
            .header(CONTENT_TYPE, "text/plain")
            .body(csr.to_string())
            .send()
            .await
    }

    async fn store_device_cert(&self, cert: String) -> Result<(), CertError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.cert_path)
            .await?;

        file.write_all(cert.as_bytes()).await?;
        file.sync_all().await?;
        file.set_permissions(Permissions::from_mode(0o444)).await?;
        Ok(())
    }
}
