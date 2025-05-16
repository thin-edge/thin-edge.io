use crate::command::Command;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use crate::warning;
use crate::CertError;
use crate::CertificateShift;
use camino::Utf8PathBuf;
use certificate::CloudHttpConfig;
use reqwest::StatusCode;
use reqwest::Url;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::TEdgeConfig;

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
    pub cloud_root_certs: CloudHttpConfig,
    pub password: String,
}

#[async_trait::async_trait]
impl Command for UploadCertCmd {
    fn description(&self) -> String {
        "upload root certificate".into()
    }

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        Ok(self.upload_certificate().await?)
    }
}

impl UploadCertCmd {
    async fn upload_certificate(&self) -> Result<(), anyhow::Error> {
        if std::env::var("C8YPASS").is_ok() {
            warning!("Detected use of a deprecated env variable, C8YPASS. Please use C8Y_PASSWORD instead\n");
        }
        let (username, password) = self.prompt_for_user_password().await?;
        let client = self.cloud_root_certs.client();

        // To post certificate c8y requires one of the following endpoints:
        // https://<tenant_id>.cumulocity.url.io[:port]/tenant/tenants/<tenant_id>/trusted-certificates
        // https://<tenant_domain>.cumulocity.url.io[:port]/tenant/tenants/<tenant_id>/trusted-certificates
        // and therefore we need to get tenant_id.
        let tenant_id = get_tenant_id(
            &client,
            build_get_tenant_id_url(&self.host.to_string())?,
            &username,
            &password,
        )
        .await?;
        self.post_certificate(&client, &tenant_id, &username, &password)
            .await?;
        Ok(())
    }

    async fn prompt_for_user_password(&self) -> Result<(String, String), anyhow::Error> {
        use std::io::Write;
        let username_arg = self.username.clone();
        let password_arg = self.password.clone();
        tokio::task::spawn_blocking(move || {
            // Prompt if not already set
            let username = if username_arg.is_empty() {
                print!("Enter username: ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                input.trim_end_matches(['\n', '\r']).to_string()
            } else {
                username_arg
            };

            // Read the password from /dev/tty
            let password = if password_arg.is_empty() {
                rpassword::read_password_from_tty(Some("Enter password: "))?
            } else {
                password_arg
            };

            Ok((username, password))
        })
        .await?
    }

    async fn post_certificate(
        &self,
        client: &reqwest::Client,
        tenant_id: &str,
        username: &str,
        password: &str,
    ) -> Result<(), CertError> {
        let (is_new, cert_path) = match CertificateShift::exists_new_certificate(&self.path).await {
            Some(certificate_shift) => (true, certificate_shift.new_cert_path),
            None => (false, self.path.to_path_buf()),
        };
        let post_url = build_upload_certificate_url(&self.host.to_string(), tenant_id)?;

        let post_body = UploadCertBody {
            auto_registration_enabled: true,
            cert_in_pem_format: tokio::fs::read_to_string(&cert_path).await?,
            name: self.device_id.clone(),
            status: "ENABLED".into(),
        };

        let res = client
            .post(post_url)
            .json(&post_body)
            .basic_auth(username, Some(password))
            .send()
            .await
            .map_err(get_webpki_error_from_reqwest)?;

        let cert_name = if is_new {
            "New device certificate"
        } else {
            "Device certificate"
        };
        match res.status() {
            StatusCode::OK | StatusCode::CREATED => {
                println!("{cert_name} uploaded successfully.");
                Ok(())
            }

            StatusCode::CONFLICT => {
                println!("{cert_name} already exists in the cloud.");
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

async fn get_tenant_id(
    client: &reqwest::Client,
    url: Url,
    username: &str,
    password: &str,
) -> Result<String, CertError> {
    let res = client
        .get(url)
        .basic_auth(username, Some(password))
        .send()
        .await
        .map_err(get_webpki_error_from_reqwest)?
        .error_for_status()?;

    let body = res.json::<CumulocityResponse>().await?;
    Ok(body.name)
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

    #[tokio::test]
    async fn get_tenant_id_should_return_error_given_wrong_credentials() {
        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::new();

        let mut server = mockito::Server::new_async().await;
        let request_url = Url::parse(&format!("{}/tenant/currentTenant", server.url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDpmYWlsZWR0ZXN0"; // Base64 encoded test:failedtest

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let _serv = server
            .mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create_async()
            .await;

        let res = get_tenant_id(&client, request_url, "test", "test").await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn get_tenant_id_returns_correct_response() {
        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::new();

        let mut server = mockito::Server::new_async().await;
        let request_url = Url::parse(&format!("{}/tenant/currentTenant", server.url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"name":"test"}"#;
        let expected_status = 200;

        let expected = "test";

        let _serv = server
            .mock("GET", "/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create_async()
            .await;

        let res = get_tenant_id(&client, request_url, "test", "test")
            .await
            .unwrap();

        assert_eq!(res, expected);
    }

    #[tokio::test]
    async fn get_tenant_id_response_should_return_error_when_response_has_no_name_field() {
        #[allow(clippy::disallowed_methods)]
        let client = reqwest::Client::new();

        let mut server = mockito::Server::new_async().await;
        let request_url = Url::parse(&format!("{}/tenant/currentTenant", server.url())).unwrap();

        let auth_header_field = "authorization";
        let auth_header_value = "Basic dGVzdDp0ZXN0"; // Base64 encoded test:test

        let response_body = r#"{"test":"test"}"#;
        let expected_status = 200;

        let _serv = server
            .mock("GET", "/test/tenant/currentTenant")
            .match_header(auth_header_field, auth_header_value)
            .with_body(response_body)
            .with_status(expected_status)
            .create_async()
            .await;

        let res = get_tenant_id(&client, request_url, "test", "test").await;
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
