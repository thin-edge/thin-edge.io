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
use rand::distr::Alphanumeric;
use rand::distr::SampleString;
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
use url::form_urlencoded;
use url::Url;
use yansi::Paint as _;

/// Length of the one-time password generated when the user provides none
///
/// Cumulocity limits the one-time password to 32 characters.
const GENERATED_ONE_TIME_PASSWORD_LEN: usize = 32;

/// Command to request and download a device certificate from Cumulocity
pub struct DownloadCertCmd {
    /// The device identifier to be used as the common name for the certificate
    pub device_id: String,

    /// The security token assigned to this device when registered to Cumulocity
    ///
    /// If empty, the user is prompted for a value when `prompt` is set,
    /// and a random one is generated otherwise.
    ///
    /// A password given by the user is kept secret: it is neither displayed
    /// nor added to the device registration URL.
    pub one_time_password: String,

    /// Prompt for the one-time password instead of generating a random one
    pub prompt: bool,

    /// Print the Cumulocity registration URL, which includes the one-time password
    pub show_registration_url: bool,

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
        // When using an HSM, ensure the private key that will sign the CSR exists. Resolved
        // before prompting for registration data so failures surface early.
        let key = if self.generate_csr {
            self.ensure_key_exists(config).await?
        } else {
            self.key.clone()
        };

        let registration = self.get_registration_data().await?;
        let common_name = registration.device_id.clone();
        let one_time_password = registration.one_time_password.clone();
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

        self.print_registration_details(&registration);

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

        // Point the CSR generation at the newly created key: `ensure_key` updated the config, but
        // this command still runs with the config read at startup. (`ensure_key` only skips the
        // config update when the configured URI points to another existing key, which can't be
        // the case here: the token held no private keys at all.)
        let mut cryptoki_config = cryptoki_config.clone();
        let uri = match &mut cryptoki_config {
            CryptokiConfig::Direct(direct) => &mut direct.uri,
            CryptokiConfig::SocketService { uri, .. } => uri,
        };
        *uri = Some(Arc::from(new_key_uri.as_str()));
        Ok(Key::Cryptoki(cryptoki_config))
    }

    /// Determine the device id and the one-time password to register the device
    ///
    /// - The device id is prompted for, unless already set on the command line or using env variables.
    /// - The one-time password is either the one provided by the user,
    ///   one prompted for, or a randomly generated one.
    async fn get_registration_data(&self) -> Result<RegistrationData, std::io::Error> {
        let self_device_id = self.device_id.clone();
        let self_one_time_password = self.one_time_password.clone();
        let self_prompt = self.prompt;
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

            let (one_time_password, generated) = if !self_one_time_password.is_empty() {
                (self_one_time_password, false)
            } else if self_prompt {
                // Read the security token from /dev/tty
                (
                    rpassword::prompt_password("Enter one-time password: ")?,
                    false,
                )
            } else {
                (generate_one_time_password(), true)
            };

            Ok(RegistrationData {
                device_id,
                one_time_password,
                generated,
            })
        })
        .await?
    }

    /// Print the details required to register the device on Cumulocity
    ///
    /// The registration URL is printed unless the user opted out.
    ///
    /// A one-time password given by the user, on the command line or on a prompt,
    /// is kept secret: it is neither displayed nor added to the registration URL.
    /// Only a password generated by this command is displayed,
    /// as the user has no other way to know it.
    fn print_registration_details(&self, registration: &RegistrationData) {
        let RegistrationData {
            device_id,
            one_time_password,
            generated,
        } = registration;

        eprintln!("Register the device on {}:\n", self.c8y_url.host());
        eprintln!("    {} {device_id}", "device id:        ".blue().bold());

        if *generated {
            eprintln!(
                "    {} {one_time_password}",
                "one-time password:".blue().bold()
            );
        }

        if self.show_registration_url {
            let password = generated.then_some(one_time_password.as_str());
            let url = registration_url(&self.c8y_url, device_id, password);
            if *generated {
                eprintln!("\n    Open the following URL to register the device:\n");
            } else {
                eprintln!(
                    "\n    Open the following URL to register the device, \
                     filling in the one-time password:\n"
                );
            }
            eprintln!("    {}", url.cyan().underline());
        }

        eprintln!("\nWaiting for the device to be registered ...\n");
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

/// The data used to register the device on Cumulocity
struct RegistrationData {
    /// The device identifier
    device_id: String,

    /// The one-time password used to authenticate the certificate request
    one_time_password: String,

    /// Tell if the one-time password has been generated by this command,
    /// in which case it has to be displayed to the user.
    ///
    /// A password given by the user is kept secret.
    generated: bool,
}

/// Generate a random one-time password
///
/// The password is URL friendly, as it is meant to be passed
/// as a query parameter of the Cumulocity device registration URL.
fn generate_one_time_password() -> String {
    loop {
        let password =
            Alphanumeric.sample_string(&mut rand::rng(), GENERATED_ONE_TIME_PASSWORD_LEN);

        // Cumulocity rejects passwords mixing neither cases nor digits
        if password.chars().any(|c| c.is_ascii_lowercase())
            && password.chars().any(|c| c.is_ascii_uppercase())
            && password.chars().any(|c| c.is_ascii_digit())
        {
            return password;
        }
    }
}

/// Build the URL of the Cumulocity device registration page,
/// pre-filled with the device id and, if not kept secret, the one-time password
///
/// The one-time password query parameter is always set,
/// left empty when the password is kept secret,
/// so the registration form highlights the field to be filled in.
fn registration_url(
    c8y_url: &HostPort<HTTPS_PORT>,
    device_id: &str,
    password: Option<&str>,
) -> String {
    let host = c8y_url.host();
    let port = c8y_url.port().0;
    let authority = if port == HTTPS_PORT {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("externalId", device_id)
        .append_pair("one-time-password", password.unwrap_or_default())
        .finish();

    format!("https://{authority}/apps/devicemanagement/index.html#/deviceregistration?{query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_password_is_url_friendly() {
        let password = generate_one_time_password();
        assert_eq!(password.len(), 32);
        assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn generated_passwords_are_random() {
        assert_ne!(generate_one_time_password(), generate_one_time_password());
    }

    #[test]
    fn generated_password_mixes_cases_and_digits() {
        for _ in 0..100 {
            let password = generate_one_time_password();
            assert!(password.chars().any(|c| c.is_ascii_lowercase()));
            assert!(password.chars().any(|c| c.is_ascii_uppercase()));
            assert!(password.chars().any(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn registration_url_is_built_from_the_c8y_host() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com").unwrap();
        assert_eq!(
            registration_url(&c8y_url, "my-device", Some("s3cr3t")),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device&one-time-password=s3cr3t"
        );
    }

    #[test]
    fn registration_url_keeps_a_non_default_port() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com:8443").unwrap();
        assert_eq!(
            registration_url(&c8y_url, "my-device", Some("s3cr3t")),
            "https://example.cumulocity.com:8443/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device&one-time-password=s3cr3t"
        );
    }

    #[test]
    fn registration_url_escapes_the_device_id_and_password() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com").unwrap();
        assert_eq!(
            registration_url(&c8y_url, "my device/01", Some("a&b=c d")),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=my+device%2F01&one-time-password=a%26b%3Dc+d"
        );
    }

    #[test]
    fn registration_url_leaves_a_secret_password_empty() {
        let c8y_url = HostPort::<HTTPS_PORT>::try_from("example.cumulocity.com").unwrap();
        assert_eq!(
            registration_url(&c8y_url, "my-device", None),
            "https://example.cumulocity.com/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device&one-time-password="
        );
    }
}
