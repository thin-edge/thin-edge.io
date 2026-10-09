//! The device side of the Cumulocity username/password registration:
//! polling the credentials request until an operator accepts it,
//! and storing the issued credentials where the mapper reads them.
//!
//! The exchanges themselves are [`c8y_api::registration`]'s.

use super::create_parent_dir;
use super::env;
use super::REGISTRATION_TIMEOUT;
use crate::cli::bootstrap::command::BootstrapCommand;
use crate::cli::bootstrap::descriptor::input_value;
use crate::cli::bootstrap::tls::tls_trust_error;
use anyhow::bail;
use anyhow::Context;
use c8y_api::registration;
use c8y_api::registration::DeviceCredentials;
use c8y_api::registration::DeviceCredentialsError;
use camino::Utf8Path;
use certificate::CloudHttpConfig;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::TEdgeConfig;
use tokio::io::AsyncWriteExt;
use tokio::time::Instant;

/// The per-request timeout of the registration exchanges
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How often a pending credentials request is polled
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Length of the generated security token (Cumulocity accepts up to 32)
const SECURITY_TOKEN_LEN: usize = 8;

/// Alphabet for generated security tokens:
/// the operator reads the token off this device's console
/// and retypes it in the UI, so look-alike characters (0/O, 1/I) are excluded
const SECURITY_TOKEN_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// The HTTP client of the registration exchanges
pub(super) fn client(http_config: &CloudHttpConfig) -> reqwest::Result<reqwest::Client> {
    http_config
        .client_builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
}

impl BootstrapCommand {
    /// Poll the device credentials request with the tenant's bootstrap user
    /// until an operator accepts the registration
    pub(super) async fn request_device_credentials(
        &self,
        config: &TEdgeConfig,
        c8y_url: &HostPort<HTTPS_PORT>,
        device_id: &str,
    ) -> anyhow::Result<DeviceCredentials> {
        let (bootstrap_user, bootstrap_password) = self.credential_inputs(
            super::method::BASIC,
            "the tenant's bootstrap credentials",
            env::BOOTSTRAP_USER,
            env::BOOTSTRAP_PASSWORD,
        )?;
        // The security token proves the credentials are handed to the device
        // the operator is looking at: the operator must enter the same value
        // in the UI when accepting the registration — but only on tenants
        // configured to demand it; everywhere else the field is ignored
        let security_token = input_value(&self.hook_envs, env::SECURITY_TOKEN)
            .unwrap_or_else(generate_security_token);
        let client = client(&config.cloud_root_certs().await?)?;
        let trust_store = self.trust_store(config).await;
        let base_url = format!("https://{c8y_url}");

        eprintln!("Waiting for the device registration to be accepted");
        eprintln!();
        eprintln!("  Open the following URL to register the device (if not already done)");
        eprintln!("  and accept the registration request while this command is polling:");
        eprintln!();
        eprintln!(
            "  {}",
            registration::device_registration_url(c8y_url, device_id, None)
        );
        eprintln!();
        eprintln!("  Device ID:      {device_id}");
        eprintln!("  Security token: {security_token}");
        eprintln!("  (enter this exact value if the UI asks for one when accepting");
        eprintln!("   the registration; it can be ignored otherwise)");

        let deadline = Instant::now() + REGISTRATION_TIMEOUT;
        let mut waiting_reported = false;
        loop {
            let response = registration::request_device_credentials(
                &client,
                &base_url,
                device_id,
                &bootstrap_user,
                &bootstrap_password,
                &security_token,
            )
            .await;
            match response {
                Ok(Some(credentials)) => return Ok(credentials),
                Ok(None) => {
                    if !waiting_reported {
                        eprintln!(
                            "Registration not accepted yet, polling every {}s...",
                            POLL_INTERVAL.as_secs()
                        );
                        waiting_reported = true;
                    }
                }
                Err(DeviceCredentialsError::Unauthorized(_)) => {
                    bail!(
                        "The bootstrap credentials were rejected by {c8y_url}. \
                         Dedicated Cumulocity instances use their own bootstrap user: \
                         set the {} and {} environment variables",
                        env::BOOTSTRAP_USER,
                        env::BOOTSTRAP_PASSWORD
                    );
                }
                Err(DeviceCredentialsError::Request(err)) => {
                    // A rejected certificate will not start being accepted
                    // by polling: report it instead of retrying to the deadline
                    if let Some(err) =
                        tls_trust_error(&err, &c8y_url.host().to_string(), &trust_store)
                    {
                        return Err(err);
                    }
                    eprintln!("Connection error ({err}), retrying...");
                }
                Err(err) => return Err(err.into()),
            }

            if Instant::now() >= deadline {
                bail!(
                    "Timed out after {}s waiting for the device registration to be accepted",
                    REGISTRATION_TIMEOUT.as_secs()
                );
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

/// Store the credentials with mode 600, owned by tedge:tedge where possible
pub async fn store_credentials(
    path: &Utf8Path,
    credentials: &DeviceCredentials,
) -> anyhow::Result<()> {
    // the serialized form carries the password too: zeroed on drop
    let content = credentials
        .to_credentials_file()
        .context("Failed to serialize the credentials")?;
    create_parent_dir(path).await?;
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
    use c8y_api::http_proxy::read_c8y_credentials;
    use certificate::Zeroizing;

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
            read_c8y_credentials(&path).unwrap(),
            ("t1234/device_demo01".to_owned(), "pw".to_owned())
        );
    }
}
