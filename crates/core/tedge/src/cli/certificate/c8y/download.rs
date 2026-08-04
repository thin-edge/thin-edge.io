use crate::cli::certificate::c8y::create_device_csr;
use crate::cli::certificate::c8y::read_csr_from_file;
use crate::cli::certificate::c8y::store_device_cert;
use crate::cli::certificate::create_csr::Key;
use crate::cli::certificate::show::ShowCertCmd;
use crate::cli::common::Cloud;
use crate::cli::hsm::CreateKeyHsmCmd;
use crate::cli::hsm::EcCurve;
use crate::cli::hsm::KeyType;
use crate::cli::hsm::RsaBits;
use crate::command::Command;
use crate::error;
use crate::get_webpki_error_from_reqwest;
use crate::log::MaybeFancy;
use crate::warning;
use anyhow::Context;
use anyhow::Error;
use c8y_api::json_c8y_deserializer::C8yAPIError;
use camino::Utf8PathBuf;
use certificate::CloudHttpConfig;
use certificate::CsrTemplate;
use hyper::StatusCode;
use reqwest::header::CONTENT_TYPE;
use reqwest::Response;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::TEdgeConfig;
use tedge_p11::service::ListKeysRequest;
use tedge_p11::CryptokiConfig;
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
    pub root_certs: CloudHttpConfig,

    /// The path where the device certificate will be stored
    pub cert_path: Utf8PathBuf,

    /// The path where the device private key will be stored
    pub key: Key,

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

    /// The user who will own the CSR file
    pub user: String,

    /// The group who will own the CSR file
    pub group: String,

    /// The cloud (and profile) the certificate is downloaded for; used to save the key URI to the
    /// right config setting if a key has to be created automatically on the HSM
    pub cloud: Option<Cloud>,
}

#[async_trait::async_trait]
impl Command for DownloadCertCmd {
    fn description(&self) -> String {
        format!(
            "Download a certificate from {} for the device {}",
            self.c8y_url, self.device_id
        )
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<Error>> {
        self.download_device_certificate(config).await?;
        eprintln!("Certificate downloaded successfully");
        eprintln!("    => the device can now be connected\n");
        ShowCertCmd::show(&self.cert_path).await?;
        Ok(())
    }
}

impl DownloadCertCmd {
    async fn download_device_certificate(&self, config: TEdgeConfig) -> Result<(), Error> {
        // Signing a CSR requires a private key; when using an HSM, create one automatically if
        // the token doesn't hold one yet, so a fresh device can be provisioned with this command
        // alone. Resolved before prompting for registration data so failures surface early.
        let key = if self.generate_csr {
            self.ensure_key_exists(config).await?
        } else {
            self.key.clone()
        };

        let (common_name, one_time_password) = self.get_registration_data().await?;
        if self.generate_csr {
            create_device_csr(
                common_name.clone(),
                key,
                None,
                self.csr_path.clone(),
                self.csr_template.clone(),
                self.user.clone(),
                self.group.clone(),
            )
            .await
            .with_context(|| format!("Fail to create the device CSR {}", self.csr_path))?;
        }
        let csr = read_csr_from_file(&self.csr_path).await?;

        let http = self
            .root_certs
            .client_builder()
            .build()
            .expect("Valid reqwest client builder configuration");
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

    /// Makes sure the private key that will sign the CSR exists, creating it if needed.
    ///
    /// For a local key this is a no-op: CSR generation creates the key file itself when missing.
    /// For an HSM-backed key, the key is created on the token (reusing the `tedge hsm create-key`
    /// logic, which also initializes the token if needed) so that a fresh device can be
    /// provisioned by this command alone. To stay predictable, a key is only created when the
    /// token verifiably holds none:
    ///
    /// - any failure to inspect the HSM (e.g. a wrong PIN) aborts with that error instead of
    ///   attempting to create anything, to not consume PIN retry attempts of hardware tokens
    /// - if the token holds keys but none matches the configured `key_uri`, or several tokens are
    ///   available, the command fails and lists the candidates rather than guessing
    async fn ensure_key_exists(&self, config: TEdgeConfig) -> Result<Key, Error> {
        let Key::Cryptoki(cryptoki_config) = &self.key else {
            return Ok(self.key.clone());
        };

        let key_uri = match cryptoki_config {
            CryptokiConfig::Direct(direct) => direct.uri.clone(),
            CryptokiConfig::SocketService { uri, .. } => uri.clone(),
        };

        let cryptoki = tedge_p11::tedge_p11_service(cryptoki_config.clone())?;

        // The signing key is looked up the same way CSR generation will: if it resolves, there is
        // nothing to do.
        let key_error = match cryptoki.get_public_key_pem(key_uri.as_deref()) {
            Ok(_) => return Ok(self.key.clone()),
            Err(err) => err,
        };

        // A tedge-p11-server that can't parse the request is too old for this lookup as well as
        // for creating a key, so report that instead of inspecting the HSM any further. Same
        // detection as in `certificate::cryptoki::RemoteKeyPair::from_cryptoki`.
        if format!("{key_error:#}").contains("Failed to parse the received frame") {
            return Err(
                anyhow::Error::from(certificate::cryptoki::CryptokiError::TooOld(key_error))
                    .context("Failed to find a private key to sign the CSR"),
            );
        }

        // No key was found. Only proceed with creating one automatically when the state of the
        // HSM is unambiguous; in every other case report the original error with context.
        let tokens = match cryptoki.list_tokens() {
            Ok(response) => response.tokens,
            // The HSM can't even be inspected, so the missing key is not the root problem.
            Err(_) => {
                return Err(key_error.context("Failed to find a private key to sign the CSR"));
            }
        };

        let initialized: Vec<_> = tokens.iter().filter(|t| t.initialized).collect();
        match initialized.as_slice() {
            // A fresh HSM without any initialized token: `create-key` initializes it and creates
            // the key, which cannot conflict with anything.
            [] => (),
            [token] => {
                // Inspecting the keys requires logging in to the token: a failure here (e.g. a
                // wrong PIN) must abort, as attempting to create a key would consume further PIN
                // retry attempts.
                let keys = cryptoki
                    .list_keys(ListKeysRequest {
                        uri: Some(token.uri.clone()),
                        pin: None,
                    })
                    .context(
                        "Failed to find a private key to sign the CSR, and the PKCS #11 token \
                         can't be inspected; not creating a key automatically",
                    )?;
                let private_keys: Vec<_> =
                    keys.keys.iter().filter(|k| k.class == "private").collect();
                // The token holds keys, but the configured URI selects none of them: choosing one
                // or creating yet another key would be a guess, so let the user decide.
                if !private_keys.is_empty() {
                    let uris = private_keys
                        .iter()
                        .map(|k| k.uri.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    anyhow::bail!(
                        "No private key matching the configured `key_uri` was found, but the \
                         PKCS #11 token holds other private keys:\n{uris}\n\
                         Select one with `tedge config set c8y.device.key_uri <URI>`, or create a \
                         new key with `tedge hsm create-key`."
                    );
                }
            }
            // Several tokens: creating a key on one of them would be a guess.
            _ => {
                let uris = initialized
                    .iter()
                    .map(|t| t.uri.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                anyhow::bail!(
                    "No private key was found to sign the CSR and multiple PKCS #11 tokens are \
                     available:\n{uris}\n\
                     Create a key on one of them with `tedge hsm create-key <TOKEN>`."
                );
            }
        }

        eprintln!("No private key was found on the PKCS #11 token; creating one automatically.");
        let create_key = CreateKeyHsmCmd {
            cryptoki_config: cryptoki_config.clone(),
            bits: RsaBits::Bits2048,
            curve: EcCurve::P256,
            label: "tedge".to_string(),
            r#type: KeyType::Ecdsa,
            id: None,
            pin: None,
            so_pin: None,
            token_label: "tedge".to_string(),
            force_new: false,
            outfile_pubkey: None,
            cloud: self.cloud.clone(),
            token: None,
        };
        let new_key_uri = create_key.ensure_key(config).await?;

        // Point the CSR generation at the newly created key. The config was updated by
        // `ensure_key` already, but this command runs with the config read at startup.
        //
        // NOTE: `ensure_key` skips the config update when the configured URI points to another
        // existing key, which would leave `tedge connect` using a key different from the one the
        // downloaded certificate is issued for. That cannot happen here: this point is only
        // reached after verifying the token holds no private keys at all.
        let mut cryptoki_config = cryptoki_config.clone();
        let uri = match &mut cryptoki_config {
            CryptokiConfig::Direct(direct) => &mut direct.uri,
            CryptokiConfig::SocketService { uri, .. } => uri,
        };
        *uri = Some(Arc::from(new_key_uri.as_str()));
        Ok(Key::Cryptoki(cryptoki_config))
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
                rpassword::prompt_password("Enter one-time password: ")?
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
