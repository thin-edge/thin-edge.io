//! Cumulocity username/password (basic auth) device registration
//!
//! Implements "Step 0: Request device credentials" over HTTP:
//! the device polls `POST /devicecontrol/deviceCredentials`
//! with the tenant's bootstrap user until an operator accepts
//! the registration request in the Device Management UI,
//! then stores the returned permanent credentials
//! at the configured `c8y.credentials_path`.

use super::tls::tls_trust_error;
use super::tls::TrustStore;
use anyhow::bail;
use anyhow::Context;
use camino::Utf8Path;
use certificate::CloudHttpConfig;
use certificate::Zeroizing;
use reqwest::header::ACCEPT;
use reqwest::header::CONTENT_TYPE;
use reqwest::StatusCode;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::time::Instant;

const CREDENTIALS_CONTENT_TYPE: &str = "application/vnd.com.nsn.cumulocity.devicecredentials+json";

/// The per-request timeout of the registration exchanges
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Length of the generated security token (Cumulocity accepts up to 32)
const SECURITY_TOKEN_LEN: usize = 8;

/// Alphabet for generated security tokens:
/// the operator reads the token off this device's console
/// and retypes it in the UI, so look-alike characters (0/O, 1/I) are excluded
const SECURITY_TOKEN_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// The permanent device credentials returned by the platform
pub struct DeviceCredentials {
    /// `<tenant-id>/<username>`, as expected by the Cumulocity MQTT/HTTP endpoints
    pub username: String,
    /// Zeroed on drop, following the mqtt_channel password convention
    pub password: Zeroizing<String>,
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

#[derive(serde::Serialize)]
struct CredentialsFile<'a> {
    c8y: CredentialsFileC8y<'a>,
}

#[derive(serde::Serialize)]
struct CredentialsFileC8y<'a> {
    username: &'a str,
    password: &'a str,
}

/// Poll the device credentials endpoint until the registration is accepted
///
/// `base_url` is the platform's HTTP base (`https://<host>`).
/// HTTP 404 means "not accepted yet, keep polling";
/// a success response carries the permanent credentials.
///
/// The bootstrap credentials are the `basic` method's declared inputs
/// (`$C8Y_BOOTSTRAP_USER` / `$C8Y_BOOTSTRAP_PASSWORD`),
/// resolved by the caller - no defaults live in the code
#[allow(clippy::too_many_arguments)]
pub async fn request_device_credentials(
    base_url: &str,
    device_id: &str,
    bootstrap_user: &str,
    bootstrap_password: &str,
    http_config: &CloudHttpConfig,
    trust_store: &TrustStore,
    retry_every: Duration,
    max_timeout: Duration,
) -> anyhow::Result<DeviceCredentials> {
    // The security token proves the credentials are handed to the device
    // the operator is looking at: the operator must enter the same value
    // in the UI when accepting the registration — but only on tenants
    // configured to demand it; everywhere else the field is ignored
    let security_token = match std::env::var("C8Y_SECURITY_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => generate_security_token(),
    };

    let client = http_config.client_builder().build()?;
    let url = format!("{base_url}/devicecontrol/deviceCredentials");
    let body = serde_json::json!({ "id": device_id, "securityToken": security_token });

    eprintln!("Waiting for the device registration to be accepted");
    eprintln!();
    eprintln!("  Open the following URL to register the device (if not already done)");
    eprintln!("  and accept the registration request while this command is polling:");
    eprintln!();
    eprintln!("  {}", registration_url(base_url, device_id));
    eprintln!();
    eprintln!("  Device ID:      {device_id}");
    eprintln!("  Security token: {security_token}");
    eprintln!("  (enter this exact value if the UI asks for one when accepting");
    eprintln!("   the registration; it can be ignored otherwise)");

    let deadline = Instant::now() + max_timeout;
    let mut waiting_reported = false;
    loop {
        let response = client
            .post(&url)
            .basic_auth(bootstrap_user, Some(bootstrap_password))
            .header(CONTENT_TYPE, CREDENTIALS_CONTENT_TYPE)
            .header(ACCEPT, CREDENTIALS_CONTENT_TYPE)
            .json(&body)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await;

        match response {
            Ok(response) if response.status().is_success() => {
                let credentials: DeviceCredentialsResponse = response
                    .json()
                    .await
                    .context("Failed to parse the device credentials response")?;
                return Ok(DeviceCredentials {
                    username: format!("{}/{}", credentials.tenant_id, credentials.username),
                    // moved, not copied: the buffer ends up zeroize-on-drop
                    password: Zeroizing::new(credentials.password),
                });
            }
            Ok(response) if response.status() == StatusCode::NOT_FOUND => {
                if !waiting_reported {
                    eprintln!(
                        "Registration not accepted yet, polling every {}s...",
                        retry_every.as_secs()
                    );
                    waiting_reported = true;
                }
            }
            Ok(response) if response.status() == StatusCode::UNAUTHORIZED => {
                bail!(
                    "The bootstrap credentials were rejected by {base_url}. \
                     Dedicated Cumulocity instances use their own bootstrap user: \
                     set the C8Y_BOOTSTRAP_USER and C8Y_BOOTSTRAP_PASSWORD environment variables"
                );
            }
            Ok(response) => {
                let status = response.status();
                let detail = response.text().await.unwrap_or_default();
                bail!("Requesting device credentials failed: HTTP {status}\n{detail}");
            }
            Err(err) => {
                // A rejected certificate will not start being accepted
                // by polling: report it instead of retrying to the deadline
                if let Some(err) = tls_trust_error(&err, host_of(base_url), trust_store) {
                    return Err(err);
                }
                eprintln!("Connection error ({err}), retrying...");
            }
        }

        if Instant::now() >= deadline {
            bail!(
                "Timed out after {}s waiting for the device registration to be accepted",
                max_timeout.as_secs()
            );
        }
        tokio::time::sleep(retry_every).await;
    }
}

/// The outcome of checking issued credentials against the platform
#[derive(Debug, PartialEq, Eq)]
pub enum CredentialsCheck {
    Verified,
    /// The platform rejected the credentials (HTTP 401)
    Rejected,
    /// The platform could not be asked (unreachable, or an unexpected status)
    Unverifiable(String),
}

/// Check issued device credentials with an authenticated no-op request
/// (`GET /user/currentUser`)
pub async fn verify_device_credentials(
    base_url: &str,
    credentials: &DeviceCredentials,
    http_config: &CloudHttpConfig,
    trust_store: &TrustStore,
) -> anyhow::Result<CredentialsCheck> {
    let client = http_config.client_builder().build()?;
    let response = client
        .get(format!("{base_url}/user/currentUser"))
        .basic_auth(&credentials.username, Some(credentials.password.as_str()))
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await;
    Ok(match response {
        Ok(response) if response.status() == StatusCode::UNAUTHORIZED => CredentialsCheck::Rejected,
        Ok(response) if response.status().is_success() => CredentialsCheck::Verified,
        Ok(response) => {
            CredentialsCheck::Unverifiable(format!("HTTP {} from {base_url}", response.status()))
        }
        // an untrusted platform is not an unverifiable one:
        // every later exchange fails the same way
        Err(err) => match tls_trust_error(&err, host_of(base_url), trust_store) {
            Some(err) => return Err(err),
            None => CredentialsCheck::Unverifiable(err.to_string()),
        },
    })
}

/// The host of a `https://<host>` base URL, as used in the error reports
fn host_of(base_url: &str) -> &str {
    base_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
}

/// Store the credentials with mode 600, owned by tedge:tedge where possible
pub async fn store_credentials(
    path: &Utf8Path,
    credentials: &DeviceCredentials,
) -> anyhow::Result<()> {
    // the serialized form carries the password too: zero it on drop
    let content = Zeroizing::new(
        toml::to_string(&CredentialsFile {
            c8y: CredentialsFileC8y {
                username: &credentials.username,
                password: &credentials.password,
            },
        })
        .context("Failed to serialize the credentials")?,
    );

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create directory {parent}"))?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true).mode(0o600);
    let mut file = options
        .open(path)
        .await
        .with_context(|| format!("Failed to create the credentials file at {path}"))?;
    file.write_all(content.as_bytes()).await?;
    file.flush().await?;

    if let Err(err) = tedge_utils::file::change_user_and_group(path, "tedge", "tedge").await {
        eprintln!("Warning: could not change the owner of {path} to tedge:tedge ({err})");
    }
    Ok(())
}

/// The username stored in a credentials file, if the file is readable
pub async fn read_stored_username(path: &Utf8Path) -> Option<String> {
    let content = Zeroizing::new(tokio::fs::read_to_string(path).await.ok()?);
    let table: toml::Table = content.parse().ok()?;
    table
        .get("c8y")?
        .get("username")?
        .as_str()
        .map(str::to_owned)
}

/// The device registration page, pre-filled with the device id.
///
/// Unlike the c8y-ca variant, no one-time password is included:
/// the basic handshake has none — the security token, when the tenant
/// demands one, is entered by the operator, never carried in a URL
fn registration_url(base_url: &str, device_id: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let base_url = base_url.strip_suffix(":443").unwrap_or(base_url);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("externalId", device_id)
        .finish();
    format!("{base_url}/apps/devicemanagement/index.html#/deviceregistration?{query}")
}

/// Generate a security token that survives being read aloud and retyped
fn generate_security_token() -> String {
    (0..SECURITY_TOKEN_LEN)
        .map(|_| {
            SECURITY_TOKEN_ALPHABET[rand::random_range(0..SECURITY_TOKEN_ALPHABET.len())] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_tokens_use_the_unambiguous_alphabet() {
        for _ in 0..20 {
            let token = generate_security_token();
            assert_eq!(token.len(), SECURITY_TOKEN_LEN);
            assert!(
                token.bytes().all(|c| SECURITY_TOKEN_ALPHABET.contains(&c)),
                "unexpected character in {token:?}"
            );
            assert!(!token.contains(['0', 'O', '1', 'I']));
        }
    }

    #[test]
    fn credentials_file_is_valid_toml_with_special_characters() {
        let content = toml::to_string(&CredentialsFile {
            c8y: CredentialsFileC8y {
                username: "t1234/device_test",
                password: "pa\"ss\\word\n",
            },
        })
        .unwrap();
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

    #[test]
    fn registration_url_carries_the_device_id_without_the_default_port() {
        assert_eq!(
            registration_url("https://example.com:443", "dev 01"),
            "https://example.com/apps/devicemanagement/index.html#/deviceregistration?externalId=dev+01"
        );
    }

    #[tokio::test]
    async fn credentials_are_polled_until_the_registration_is_accepted() {
        let mut server = mockito::Server::new_async().await;
        let pending = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .match_header("authorization", "Basic Ym9vdDpzZWNyZXQ=") // boot:secret
            .with_status(404)
            .expect(1)
            .create_async()
            .await;
        let accepted = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(201)
            .with_body(
                r#"{"id":"demo01","tenantId":"t1234","username":"device_demo01","password":"pw"}"#,
            )
            .expect(1)
            .create_async()
            .await;

        let credentials = request_device_credentials(
            &server.url(),
            "demo01",
            "boot",
            "secret",
            &CloudHttpConfig::test_value(),
            &TrustStore::test_value(),
            Duration::from_millis(10),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(credentials.username, "t1234/device_demo01");
        assert_eq!(credentials.password.as_str(), "pw");
        pending.assert_async().await;
        accepted.assert_async().await;
    }

    #[tokio::test]
    async fn rejected_bootstrap_credentials_fail_immediately() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(401)
            .create_async()
            .await;
        let err = request_device_credentials(
            &server.url(),
            "demo01",
            "boot",
            "wrong",
            &CloudHttpConfig::test_value(),
            &TrustStore::test_value(),
            Duration::from_millis(10),
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("rejected"), "{err}");
    }

    #[tokio::test]
    async fn pending_registration_times_out() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/devicecontrol/deviceCredentials")
            .with_status(404)
            .create_async()
            .await;
        let err = request_device_credentials(
            &server.url(),
            "demo01",
            "boot",
            "secret",
            &CloudHttpConfig::test_value(),
            &TrustStore::test_value(),
            Duration::from_millis(10),
            Duration::from_millis(50),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Timed out"), "{err}");
    }

    #[tokio::test]
    async fn issued_credentials_are_verified_against_the_platform() {
        let credentials = DeviceCredentials {
            username: "t1234/device_demo01".into(),
            password: Zeroizing::new("pw".into()),
        };
        let http = CloudHttpConfig::test_value();

        let mut server = mockito::Server::new_async().await;
        let ok = server
            .mock("GET", "/user/currentUser")
            .with_status(200)
            .create_async()
            .await;
        assert_eq!(
            verify_device_credentials(
                &server.url(),
                &credentials,
                &http,
                &TrustStore::test_value(),
            )
            .await
            .unwrap(),
            CredentialsCheck::Verified
        );
        ok.remove_async().await;

        let rejected = server
            .mock("GET", "/user/currentUser")
            .with_status(401)
            .create_async()
            .await;
        assert_eq!(
            verify_device_credentials(
                &server.url(),
                &credentials,
                &http,
                &TrustStore::test_value(),
            )
            .await
            .unwrap(),
            CredentialsCheck::Rejected
        );
        rejected.remove_async().await;

        let _outage = server
            .mock("GET", "/user/currentUser")
            .with_status(503)
            .create_async()
            .await;
        assert!(matches!(
            verify_device_credentials(
                &server.url(),
                &credentials,
                &http,
                &TrustStore::test_value(),
            )
            .await
            .unwrap(),
            CredentialsCheck::Unverifiable(_)
        ));
    }

    #[tokio::test]
    async fn stored_credentials_are_private_and_readable_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = Utf8Path::from_path(tmp.path())
            .unwrap()
            .join("credentials.toml");
        let credentials = DeviceCredentials {
            username: "t1234/device_demo01".into(),
            password: Zeroizing::new("pw".into()),
        };
        store_credentials(&path, &credentials).await.unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(
            read_stored_username(&path).await.as_deref(),
            Some("t1234/device_demo01")
        );
        assert_eq!(read_stored_username(&path.join("missing")).await, None);
    }
}
