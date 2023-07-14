use super::error::get_webpki_error_from_reqwest;
use super::error::CertError;
use crate::command::Command;
use camino::Utf8PathBuf;
use reqwest::StatusCode;
use reqwest::Url;
use std::io::prelude::*;
use std::path::Path;
use tedge_config::get_new_tedge_config;
use tedge_config::HostPort;
use tedge_config::HTTPS_PORT;

#[derive(Debug, serde::Deserialize)]
struct CumulocityResponse {
    name: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadCertBody {
    name: String,
    cert_in_pem_format: String,
    auto_registration_enabled: bool,
    status: String,
}

pub struct UploadCertCmd {
    pub device_id: String,
    pub path: Utf8PathBuf,
    pub host: HostPort<HTTPS_PORT>,
    pub username: String,
}

impl Command for UploadCertCmd {
    fn description(&self) -> String {
        "upload root certificate".into()
    }

    fn execute(&self) -> anyhow::Result<()> {
        Ok(self.upload_certificate()?)
    }
}

impl UploadCertCmd {
    fn upload_certificate(&self) -> Result<(), CertError> {
        // Read the password from /dev/tty
        // Unless a password is provided using the `C8YPASS` env var.
        let password = match std::env::var("C8YPASS") {
            Ok(password) => password,
            Err(_) => rpassword::read_password_from_tty(Some("Enter password: "))?,
        };

        let config = get_new_tedge_config()?;
        let root_cert = &config.c8y.root_cert_path;
        let client_builder = reqwest::blocking::Client::builder();
        let client = match std::fs::metadata(root_cert)?.is_file() {
            true => {
                let cert = std::fs::read(root_cert)?;
                let cert_pem = reqwest::Certificate::from_pem(&cert)?;
                client_builder.add_root_certificate(cert_pem).build()?
            }
            false => client_builder.build()?,
        };

        // To post certificate c8y requires one of the following endpoints:
        // https://<tenant_id>.cumulocity.url.io[:port]/tenant/tenants/<tenant_id>/trusted-certificates
        // https://<tenant_domain>.cumulocity.url.io[:port]/tenant/tenants/<tenant_id>/trusted-certificates
        // and therefore we need to get tenant_id.
        let tenant_id = get_tenant_id_blocking(
            &client,
            build_get_tenant_id_url(self.host.as_str())?,
            &self.username,
            &password,
        )?;
        self.post_certificate(&client, &tenant_id, &password)
    }

    fn post_certificate(
        &self,
        client: &reqwest::blocking::Client,
        tenant_id: &str,
        password: &str,
    ) -> Result<(), CertError> {
        let post_url = build_upload_certificate_url(self.host.as_str(), tenant_id)?;

        let post_body = UploadCertBody {
            auto_registration_enabled: true,
            cert_in_pem_format: read_cert_to_string(&self.path)?,
            name: self.device_id.clone(),
            status: "ENABLED".into(),
        };

        let res = client
            .post(post_url)
            .json(&post_body)
            .basic_auth(&self.username, Some(password))
            .send()
            .map_err(get_webpki_error_from_reqwest)?;

        match res.status() {
            StatusCode::OK | StatusCode::CREATED => {
                println!("Certificate uploaded successfully.");
                Ok(())
            }

            StatusCode::CONFLICT => {
                println!("Certificate already exists in the cloud.");
                Ok(())
            }

            status_code => {
                println!("Something went wrong: {}", status_code);
                Err(CertError::StatusCode(status_code))
            }
        }
    }
}

fn build_get_tenant_id_url(host: &str) -> Result<Url, CertError> {
    let url = format!("https://{}/tenant/currentTenant", host);
    Ok(Url::parse(&url)?)
}

fn build_upload_certificate_url(host: &str, tenant_id: &str) -> Result<Url, CertError> {
    let url_str = format!(
        "https://{}/tenant/tenants/{}/trusted-certificates",
        host, tenant_id
    );

    Ok(Url::parse(&url_str)?)
}

fn get_tenant_id_blocking(
    client: &reqwest::blocking::Client,
    url: Url,
    username: &str,
    password: &str,
) -> Result<String, CertError> {
    let res = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()
        .map_err(get_webpki_error_from_reqwest)?
        .error_for_status()?;

    let body = res.json::<CumulocityResponse>()?;
    Ok(body.name)
}

fn read_cert_to_string(path: impl AsRef<Path>) -> Result<String, CertError> {
    let mut file = std::fs::File::open(path.as_ref()).map_err(|err| {
        let path = path.as_ref().display().to_string();
        CertError::CertificateReadFailed(err, path)
    })?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn build_get_tenant_id_url_should_return_error_given_invalid_host() {
        let result = build_get_tenant_id_url("%");

        assert!(result.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_should_return_error_given_wrong_credentials() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDpmYWlsZWR0ZXN0"; // Base64 encoded test:failedtest

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn get_tenant_id_blocking_returns_correct_response() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let expected = "test";

        let _serv = mockito::mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test").unwrap();

        assert_eq!(res, expected);
    }

    #[test]
    fn get_tenant_id_blocking_response_should_return_error_when_response_has_no_name_field() {
        let client = reqwest::blocking::Client::new();

        let request_url =
            Url::parse(&format!("{}/tenant/currentTenant", mockito::server_url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"test":"test"}"#;
        let expected_status = 200;

        let _serv = mockito::mock("GET", "/test/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create();

        let res = get_tenant_id_blocking(&client, request_url, "test", "test");
        assert!(res.is_err());
    }

    #[test]
    fn make_upload_certificate_url_correct_parameters_return_url() {
        let url = "test";
        let tenant_id = "test";
        let expected =
            reqwest::Url::parse("https://test/tenant/tenants/test/trusted-certificates").unwrap();

        let result = build_upload_certificate_url(url, tenant_id).unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn make_upload_certificate_returns_error_given_incorrect_url() {
        let url = "@";
        let tenant_id = "test";

        let result = build_upload_certificate_url(url, tenant_id).unwrap_err();
        assert_matches!(result, CertError::UrlParseError(url::ParseError::EmptyHost));
    }
}
