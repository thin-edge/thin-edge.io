use crate::cli::certificate::c8y::create_device_csr;
use crate::cli::certificate::c8y::store_device_cert;
use crate::command::Command;
use crate::error;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use anyhow::Error;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use hyper::header::AUTHORIZATION;
use hyper::header::CONTENT_TYPE;
use hyper::StatusCode;
use reqwest::blocking::Response;
use tedge_config::HostPort;
use tedge_config::HTTPS_PORT;
use tedge_config::MQTT_TLS_PORT;
use url::Url;

/// Command to renew a device certificate from Cumulocity
pub struct RenewCertCmd {
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,

    /// Cumulocity MQTT end-point where the device is authenticated
    pub c8y_mqtt: HostPort<MQTT_TLS_PORT>,

    /// Cumulocity instance from where the device got his current certificate
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

impl Command for RenewCertCmd {
    fn description(&self) -> String {
        format!("Renew the device certificate from {}", self.c8y_url)
    }

    fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        Ok(self.renew_device_certificate()?)
    }
}

impl RenewCertCmd {
    fn renew_device_certificate(&self) -> Result<(), Error> {
        let csr = create_device_csr(
            self.device_id.clone(),
            self.key_path.clone(),
            self.csr_path.clone(),
        )?;

        let jwt_token = self.get_jwt_token();

        let http = self.root_certs.blocking_client();
        let url = format!("https://{}/.well-known/est/simpleenroll", self.c8y_url);
        let url = Url::parse(&url)?;
        let result = self.post_device_csr(&http, &url, &jwt_token, &csr);
        match result {
            Ok(response) if response.status() == StatusCode::OK => {
                if let Ok(cert) = response.text() {
                    store_device_cert(&self.cert_path, cert)?;
                    return Ok(());
                }
                error!(
                    "Fail to extract a certificate from the response returned by {}",
                    self.c8y_url
                );
            }
            Ok(response) => {
                error!(
                    "The device certificate cannot be renewed on {}: {:?}",
                    self.c8y_url,
                    response.status()
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

        Ok(())
    }

    /// Post the device CSR
    fn post_device_csr(
        &self,
        http: &reqwest::blocking::Client,
        url: &Url,
        jwt_token: &str,
        csr: &str,
    ) -> Result<Response, reqwest::Error> {
        http.post(url.clone())
            .header(AUTHORIZATION, format!("Bearer {jwt_token}"))
            .header(CONTENT_TYPE, "text/plain")
            .body(csr.to_string())
            .send()
    }

    fn get_jwt_token(&self) -> String {
        let _ = self.c8y_mqtt.clone();
        todo!()
    }
}
