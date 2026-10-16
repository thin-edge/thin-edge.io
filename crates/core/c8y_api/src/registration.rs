//! Device onboarding endpoints of Cumulocity
//!
//! * the device registration page of the Device Management application,
//! * the tenant `loginOptions` query, whose `self` link reveals
//!   the tenant's canonical domain (its MQTT endpoint),
//! * the device credentials request of the username/password registration
//!   ("Step 0: Request device credentials" of the device integration guide).
//!
//! These functions perform one exchange each and leave polling,
//! retries and reporting to the caller.

use crate::http_proxy::c8y_credentials_toml;
use reqwest::header::ACCEPT;
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use url::form_urlencoded;
use url::Url;
use zeroize::Zeroizing;

const DEVICE_CREDENTIALS_CONTENT_TYPE: &str =
    "application/vnd.com.nsn.cumulocity.devicecredentials+json";

/// The device registration page, pre-filled with the device id
/// and, for a certificate registration, the one-time password.
///
/// The one-time password parameter is only added for the Cumulocity CA flow:
/// `Some("")` keeps a secret password out of the URL while still
/// highlighting the field to fill in; `None` omits the parameter altogether,
/// as the username/password registration has no one-time password
pub fn device_registration_url(
    c8y_url: &HostPort<HTTPS_PORT>,
    device_id: &str,
    one_time_password: Option<&str>,
) -> String {
    let host = c8y_url.host();
    let port = c8y_url.port().0;
    let authority = if port == HTTPS_PORT {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("externalId", device_id);
    if let Some(password) = one_time_password {
        query.append_pair("one-time-password", password);
    }
    let query = query.finish();
    format!("https://{authority}/apps/devicemanagement/index.html#/deviceregistration?{query}")
}

#[derive(thiserror::Error, Debug)]
pub enum LoginOptionsError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),

    #[error("The URL may not be correct, or may point to a non-Cumulocity instance")]
    UnexpectedStatus(#[source] reqwest::Error),

    #[error("No self link in the loginOptions response")]
    NoSelfLink,

    #[error("Invalid self link in the loginOptions response: {0}")]
    InvalidSelfLink(String),
}

/// Query the tenant's login options to discover its canonical domain.
///
/// The `self` link of the response points at the tenant's canonical domain,
/// which is the MQTT endpoint when the tenant is served
/// through a separate HTTP domain (e.g. behind an enterprise gateway).
///
/// `http_url` is the platform's HTTP base URL, e.g. `https://<host>`.
pub async fn discover_tenant_domain(
    client: &reqwest::Client,
    http_url: &str,
) -> Result<String, LoginOptionsError> {
    let response: serde_json::Value = client
        .get(format!("{http_url}/tenant/loginOptions"))
        .send()
        .await?
        .error_for_status()
        .map_err(LoginOptionsError::UnexpectedStatus)?
        .json()
        .await?;
    let self_link = response
        .get("self")
        .and_then(|v| v.as_str())
        .ok_or(LoginOptionsError::NoSelfLink)?;
    Url::parse(self_link)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .ok_or_else(|| LoginOptionsError::InvalidSelfLink(self_link.to_owned()))
}

/// The permanent device credentials issued by the platform
pub struct DeviceCredentials {
    /// `<tenant-id>/<username>`, as expected by the Cumulocity MQTT/HTTP endpoints
    pub username: String,
    /// Zeroed on drop
    pub password: Zeroizing<String>,
}

impl DeviceCredentials {
    /// The content of a credentials file the mappers read (`[c8y]` section)
    pub fn to_credentials_file(&self) -> Result<Zeroizing<String>, toml::ser::Error> {
        c8y_credentials_toml(&self.username, &self.password).map(Zeroizing::new)
    }
}

impl std::fmt::Debug for DeviceCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceCredentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCredentialsResponse {
    tenant_id: String,
    username: String,
    password: String,
}

#[derive(thiserror::Error, Debug)]
pub enum DeviceCredentialsError {
    #[error("The bootstrap credentials were rejected by {0}")]
    Unauthorized(String),

    #[error("Requesting device credentials failed: HTTP {status}\n{detail}")]
    UnexpectedStatus { status: StatusCode, detail: String },

    #[error("Failed to parse the device credentials response")]
    InvalidResponse(#[source] reqwest::Error),

    /// The request did not reach the platform
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

/// Request the device credentials with the tenant's bootstrap user.
///
/// `Ok(None)` means the registration request has not been accepted yet
/// (the platform answers 404 until an operator accepts it), so the caller
/// keeps polling; a success response carries the permanent credentials.
///
/// The security token proves the credentials are handed to the device
/// the operator is looking at: the operator enters the same value in the UI
/// when accepting the registration, on tenants configured to demand it.
///
/// `base_url` is the platform's HTTP base URL, e.g. `https://<host>`.
pub async fn request_device_credentials(
    client: &reqwest::Client,
    base_url: &str,
    device_id: &str,
    bootstrap_user: &str,
    bootstrap_password: &str,
    security_token: &str,
) -> Result<Option<DeviceCredentials>, DeviceCredentialsError> {
    let body = serde_json::json!({ "id": device_id, "securityToken": security_token });
    let response = client
        .post(format!("{base_url}/devicecontrol/deviceCredentials"))
        .basic_auth(bootstrap_user, Some(bootstrap_password))
        .header(CONTENT_TYPE, DEVICE_CREDENTIALS_CONTENT_TYPE)
        .header(ACCEPT, DEVICE_CREDENTIALS_CONTENT_TYPE)
        .json(&body)
        .send()
        .await?;
    match response.status() {
        status if status.is_success() => {
            let credentials: DeviceCredentialsResponse = response
                .json()
                .await
                .map_err(DeviceCredentialsError::InvalidResponse)?;
            Ok(Some(DeviceCredentials {
                username: format!("{}/{}", credentials.tenant_id, credentials.username),
                password: Zeroizing::new(credentials.password),
            }))
        }
        // the platform answers 404 with a JSON error while the request is
        // pending; a 404 page from some other server is a wrong URL
        StatusCode::NOT_FOUND if is_json(&response) => Ok(None),
        StatusCode::UNAUTHORIZED => Err(DeviceCredentialsError::Unauthorized(base_url.to_owned())),
        status => Err(DeviceCredentialsError::UnexpectedStatus {
            status,
            detail: response.text().await.unwrap_or_default(),
        }),
    }
}

fn is_json(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.contains("json"))
}

/// The outcome of checking issued credentials against the platform
#[derive(Debug, PartialEq, Eq)]
pub enum CredentialsCheck {
    Verified,
    /// The platform rejected the credentials (HTTP 401)
    Rejected,
    /// The platform answered with an unexpected status
    UnexpectedStatus(StatusCode),
}

/// Check issued device credentials with an authenticated no-op request
/// (`GET /user/currentUser`); a request that does not reach the platform is an error
pub async fn verify_device_credentials(
    client: &reqwest::Client,
    base_url: &str,
    credentials: &DeviceCredentials,
) -> Result<CredentialsCheck, reqwest::Error> {
    let response = client
        .get(format!("{base_url}/user/currentUser"))
        .basic_auth(&credentials.username, Some(credentials.password.as_str()))
        .send()
        .await?;
    Ok(match response.status() {
        StatusCode::UNAUTHORIZED => CredentialsCheck::Rejected,
        status if status.is_success() => CredentialsCheck::Verified,
        status => CredentialsCheck::UnexpectedStatus(status),
    })
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    async fn credentials_on(server: &mockito::Server) -> Option<DeviceCredentials> {
        request_device_credentials(
            &reqwest::Client::new(),
            &server.url(),
            "demo01",
            "boot",
            "secret",
            "TOKEN",
        )
        .await
        .unwrap()
    }

    #[test]
    fn registration_url_is_built_from_the_c8y_host() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com").unwrap();
        assert_eq!(
            device_registration_url(&c8y_url, "my-device", Some("s3cr3t")),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device&one-time-password=s3cr3t"
        );
    }

    #[test]
    fn registration_url_keeps_a_non_default_port() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com:8443").unwrap();
        assert_eq!(
            device_registration_url(&c8y_url, "my-device", Some("s3cr3t")),
            "https://example.cumulocity.com:8443/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device&one-time-password=s3cr3t"
        );
    }

    #[test]
    fn registration_url_escapes_the_device_id_and_password() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com").unwrap();
        assert_eq!(
            device_registration_url(&c8y_url, "my device/01", Some("a&b=c d")),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=my+device%2F01&one-time-password=a%26b%3Dc+d"
        );
    }

    #[test]
    fn registration_url_without_a_one_time_password_omits_the_parameter() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com:443").unwrap();
        assert_eq!(
            device_registration_url(&c8y_url, "dev 01", None),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=dev+01"
        );
        assert_eq!(
            device_registration_url(&c8y_url, "dev", Some("")),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=dev&one-time-password="
        );
    }

    #[tokio::test]
    async fn tenant_domain_is_discovered_from_the_login_options_self_link() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/tenant/loginOptions")
            .with_status(200)
            .with_body(r#"{"self": "https://t1234.eu-latest.cumulocity.com/tenant/loginOptions"}"#)
            .create_async()
            .await;
        let host = discover_tenant_domain(&reqwest::Client::new(), &server.url())
            .await
            .unwrap();
        assert_eq!(host, "t1234.eu-latest.cumulocity.com");
    }

    #[tokio::test]
    async fn non_cumulocity_endpoints_fail_discovery_with_guidance() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", "/tenant/loginOptions")
            .with_status(404)
            .create_async()
            .await;
        let err = discover_tenant_domain(&reqwest::Client::new(), &server.url())
            .await
            .unwrap_err();
        assert!(
            matches!(err, LoginOptionsError::UnexpectedStatus(_)),
            "{err}"
        );
    }

    #[test]
    fn credentials_file_is_valid_toml_with_special_characters() {
        let credentials = DeviceCredentials {
            username: "t1234/device_test".into(),
            password: Zeroizing::new("pa\"ss\\word\n".into()),
        };
        let content = credentials.to_credentials_file().unwrap();
        let parsed: toml::Value = toml::from_str(&content).unwrap();
        assert_eq!(
            parsed["c8y"]["password"].as_str().unwrap(),
            "pa\"ss\\word\n"
        );
        assert_eq!(
            parsed["c8y"]["username"].as_str().unwrap(),
            "t1234/device_test"
        );
    }

    #[tokio::test]
    async fn a_pending_registration_yields_no_credentials_yet() {
        let mut server = mockito::Server::new_async().await;
        let pending = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .match_header("authorization", "Basic Ym9vdDpzZWNyZXQ=") // boot:secret
            .with_status(404)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":"devicecontrol/Not Found"}"#)
            .create_async()
            .await;
        assert!(credentials_on(&server).await.is_none());
        pending.assert_async().await;

        // a 404 page from a server that is not Cumulocity is a wrong URL
        pending.remove_async().await;
        let _not_c8y = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(404)
            .with_header("content-type", "text/html")
            .create_async()
            .await;
        let err = request_device_credentials(
            &reqwest::Client::new(),
            &server.url(),
            "demo01",
            "boot",
            "secret",
            "TOKEN",
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, DeviceCredentialsError::UnexpectedStatus { .. }),
            "{err}"
        );
    }

    #[tokio::test]
    async fn an_accepted_registration_yields_the_issued_credentials() {
        let mut server = mockito::Server::new_async().await;
        let _accepted = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(201)
            .with_body(
                r#"{"id":"demo01","tenantId":"t1234","username":"device_demo01","password":"pw"}"#,
            )
            .create_async()
            .await;
        let credentials = credentials_on(&server).await.unwrap();
        assert_eq!(credentials.username, "t1234/device_demo01");
        assert_eq!(credentials.password.as_str(), "pw");
    }

    #[tokio::test]
    async fn rejected_bootstrap_credentials_are_an_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(401)
            .create_async()
            .await;
        let err = request_device_credentials(
            &reqwest::Client::new(),
            &server.url(),
            "demo01",
            "boot",
            "wrong",
            "TOKEN",
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, DeviceCredentialsError::Unauthorized(_)),
            "{err}"
        );
    }

    #[tokio::test]
    async fn issued_credentials_are_verified_against_the_platform() {
        let credentials = DeviceCredentials {
            username: "t1234/device_demo01".into(),
            password: Zeroizing::new("pw".into()),
        };
        let mut server = mockito::Server::new_async().await;
        let client = reqwest::Client::new();
        for (status, expected) in [
            (200, CredentialsCheck::Verified),
            (401, CredentialsCheck::Rejected),
            (
                503,
                CredentialsCheck::UnexpectedStatus(StatusCode::SERVICE_UNAVAILABLE),
            ),
        ] {
            let mock = server
                .mock("GET", "/user/currentUser")
                .with_status(status)
                .create_async()
                .await;
            let check = verify_device_credentials(&client, &server.url(), &credentials)
                .await
                .unwrap();
            assert_eq!(check, expected);
            mock.remove_async().await;
        }
    }
}
