//! Cumulocity username/password (basic auth) device registration
//!
//! Implements "Step 0: Request device credentials" over HTTP:
//! the device polls `POST /devicecontrol/deviceCredentials`
//! with the tenant's bootstrap user until an operator accepts
//! the registration request in the Device Management UI,
//! then stores the returned permanent credentials
//! at the configured `c8y.credentials_path`.

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
/// HTTP 404 means "not accepted yet, keep polling";
/// a success response carries the permanent credentials.
///
/// The bootstrap credentials are the `basic` method's declared inputs
/// (`$C8Y_BOOTSTRAP_USER` / `$C8Y_BOOTSTRAP_PASSWORD`),
/// resolved by the caller - no defaults live in the code
pub async fn request_device_credentials(
    http_host: &str,
    device_id: &str,
    bootstrap_user: &str,
    bootstrap_password: &str,
    http_config: &CloudHttpConfig,
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
    let url = format!("https://{http_host}/devicecontrol/deviceCredentials");
    let body = serde_json::json!({ "id": device_id, "securityToken": security_token });

    eprintln!("Waiting for the device registration to be accepted");
    eprintln!();
    eprintln!("  Open the following URL to register the device (if not already done)");
    eprintln!("  and accept the registration request while this command is polling:");
    eprintln!();
    eprintln!("  {}", registration_url(http_host, device_id));
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
            .timeout(Duration::from_secs(30))
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
                    "The bootstrap credentials were rejected by {http_host}. \
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

/// The device registration page, pre-filled with the device id.
///
/// Unlike the c8y-ca variant, no one-time password is included:
/// the basic handshake has none — the security token, when the tenant
/// demands one, is entered by the operator, never carried in a URL
fn registration_url(http_host: &str, device_id: &str) -> String {
    let authority = http_host.strip_suffix(":443").unwrap_or(http_host);
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("externalId", device_id)
        .finish();
    format!("https://{authority}/apps/devicemanagement/index.html#/deviceregistration?{query}")
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
}
