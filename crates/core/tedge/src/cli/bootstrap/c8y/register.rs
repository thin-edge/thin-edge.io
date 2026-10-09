//! The built-in Cumulocity registration methods:
//! `c8y-ca`, `self-signed`, `basic` and `basic-preregistered`

use super::basic;
use super::create_parent_dir;
use super::env;
use super::method;
use super::REGISTRATION_TIMEOUT;
use crate::cli::bootstrap::command::missing_device_id_error;
use crate::cli::bootstrap::command::BootstrapCommand;
use crate::cli::bootstrap::descriptor::input_value;
use crate::cli::bootstrap::mapper_toml::instance_cert_path;
use crate::cli::bootstrap::mapper_toml::instance_credentials_path;
use crate::cli::bootstrap::mapper_toml::instance_csr_path;
use crate::cli::bootstrap::mapper_toml::MapperToml;
use crate::cli::bootstrap::resolve::RegistrationMethod;
use crate::cli::bootstrap::settings::key;
use crate::cli::bootstrap::settings::KeyValue;
use crate::cli::bootstrap::tls::tls_trust_error;
use crate::cli::certificate::c8y::DownloadCertCmd;
use crate::cli::certificate::certificate_owner;
use crate::cli::certificate::create_csr::Key;
use crate::cli::certificate::csr_template;
use crate::cli::certificate::DownloadCertCli;
use crate::cli::certificate::TEdgeCertCli;
use crate::cli::certificate::UploadCertCli;
use crate::cli::common::Cloud;
use crate::cli::common::CloudArg;
use crate::command::BuildCommand;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Context;
use c8y_api::http_proxy::read_c8y_credentials;
use c8y_api::registration;
use c8y_api::registration::DeviceCredentials;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::Zeroizing;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::tedge_toml::models::auth_method::AuthMethod;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;

/// How often a pending c8y-ca registration is polled
const CA_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Where a Cumulocity instance keeps its basic-auth state:
/// the default instance and profiles in the tedge config,
/// custom-named instances in their mapper.toml
struct BasicAuthTarget {
    credentials_path: Utf8PathBuf,
    /// The device id already configured for the instance
    configured_device_id: Option<String>,
    /// The platform's HTTP host, when resolvable from the configuration
    http_host: Option<HostPort<HTTPS_PORT>>,
    /// The credentials path is bootstrap's choice and must be persisted
    /// (false when the user configured `c8y.credentials_path` explicitly)
    persist_credentials_path: bool,
}

/// How basic-auth credentials are obtained
#[derive(Clone, Copy, PartialEq, Eq)]
enum BasicAuthSource {
    /// Requested via the tenant's bootstrap user, polling until an
    /// operator accepts the registration
    Requested,
    /// Issued out of band and supplied as inputs (no cloud exchange,
    /// so this also works offline)
    Preregistered,
}

impl BootstrapCommand {
    /// Register with one of the built-in Cumulocity methods
    pub(in crate::cli::bootstrap) async fn register_builtin(
        &self,
        config: TEdgeConfig,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        // The certificate methods need the cloud: defer obtaining the
        // certificate, but persist the device identity - it is local
        // configuration, and the staged services need it.
        // The basic methods handle offline themselves (their auth switch
        // and credentials path are local configuration too)
        if self.offline
            && !matches!(
                self.register,
                RegistrationMethod::Basic | RegistrationMethod::BasicPreregistered
            )
        {
            self.ui.line(
                "offline: registration deferred - \
                 re-run this command once the device is online",
            );
            if let Some(device_id) = &self.device_id {
                self.apply_instance_updates(&[KeyValue::new(key::DEVICE_ID, device_id)])
                    .await?;
            }
            return Ok(());
        }

        match self.register {
            RegistrationMethod::C8yCa => match self.custom_mapper_name() {
                Some(name) => self.register_c8y_ca_named(name, &config).await,
                None => self.register_c8y_ca(config).await,
            },
            RegistrationMethod::SelfSigned => self.register_self_signed(config).await,
            RegistrationMethod::Basic => {
                self.register_basic_auth(&config, BasicAuthSource::Requested)
                    .await
            }
            RegistrationMethod::BasicPreregistered => {
                self.register_basic_auth(&config, BasicAuthSource::Preregistered)
                    .await
            }
            RegistrationMethod::Hook { .. } => unreachable!("hook methods never reach here"),
        }
    }

    /// The device certificate of a built-in cloud instance.
    ///
    /// On a dry run a not-yet-created profile cannot be resolved
    /// (the configure step did not persist it): `None`, with a note
    fn builtin_cert_path(
        &self,
        config: &TEdgeConfig,
    ) -> Result<Option<Utf8PathBuf>, MaybeFancy<anyhow::Error>> {
        match config.device_cert_path(Some(&self.cloud)) {
            Ok(path) => Ok(Some(path.into())),
            Err(_) if self.dry_run => {
                self.ui
                    .line("would register once the configure step has created the profile");
                Ok(None)
            }
            Err(err) => Err(anyhow::Error::new(err).into()),
        }
    }

    /// Request a certificate from the Cumulocity CA for the default
    /// instance or a profile, via the existing `tedge cert download c8y`
    async fn register_c8y_ca(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let Some(cert_path) = self.builtin_cert_path(&config)? else {
            return Ok(());
        };
        if cert_path.exists() {
            self.detail(&format!(
                "certificate already present at {cert_path}, skipping"
            ));
            return Ok(());
        }
        if self.dry_run {
            self.ui.line(&format!(
                "would request a certificate from the Cumulocity CA (stored at {cert_path})"
            ));
            return Ok(());
        }
        create_parent_dir(&cert_path).await?;
        // Resolved here rather than left to the cert download,
        // which would otherwise prompt for it on stdin:
        // no prompting once the pipeline has started
        let device_id = match &self.device_id {
            Some(device_id) => device_id.clone(),
            None => self
                .c8y_config(&config)?
                .device
                .id()
                .map_err(|_| missing_device_id_error(method::CA))?,
        };
        let cert_cli = TEdgeCertCli::Download(DownloadCertCli::C8y {
            id: device_id,
            one_time_password: self
                .one_time_password
                .value()
                .unwrap_or_default()
                .to_owned(),
            show_one_time_password: self.one_time_password.generated().is_some(),
            prompt: false,
            no_registration_url: false,
            profile: self.profile().cloned(),
            csr_path: None,
            url: None,
            retry_every: CA_POLL_INTERVAL,
            max_timeout: REGISTRATION_TIMEOUT,
        });
        let cmd = cert_cli
            .build_command(&config)
            .await
            .map_err(|e| anyhow!(e))?;
        cmd.execute(config).await
    }

    /// Cumulocity CA registration for a custom-named c8y instance.
    ///
    /// Each Cumulocity instance needs its own CA-signed public certificate
    /// (each tenant's CA signs its own), stored under the instance's mapper
    /// directory; the private key is shared with the default instance.
    async fn register_c8y_ca_named(
        &self,
        name: &str,
        config: &TEdgeConfig,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let mapper_dir = self.mapper_dir(name);
        let cert_path = instance_cert_path(&mapper_dir);
        let key_path: Utf8PathBuf = config
            .device_key_path(None::<&Cloud>)
            .map_err(anyhow::Error::new)?
            .into();

        if cert_path.exists() {
            self.detail(&format!(
                "certificate already present at {cert_path}, skipping"
            ));
        } else if self.dry_run {
            self.ui.line(&format!(
                "would request a certificate from the Cumulocity CA, \
                 stored at {cert_path} (using the shared device key)"
            ));
        } else {
            create_parent_dir(&cert_path).await?;
            let c8y_url = self.named_instance_c8y_url(name).await?;
            // Resolved here rather than left to the cert download,
            // which would otherwise prompt for it on stdin
            let device_id = match &self.device_id {
                Some(device_id) => device_id.clone(),
                None => MapperToml::load_or_empty(&self.mapper_config_path(name))
                    .await
                    .device_id()
                    .map(str::to_owned)
                    .ok_or_else(|| missing_device_id_error(method::CA))?,
            };
            // The same key resolution as `tedge cert download c8y`:
            // the HSM-backed key when cryptoki is configured,
            // else the shared private key file
            let key = match config.device.cryptoki_config(None::<&dyn CloudConfig>)? {
                Some(cryptoki) => Key::Cryptoki(cryptoki),
                None => Key::Local(key_path.clone()),
            };
            let (user, group) = certificate_owner(config);
            let cmd = DownloadCertCmd {
                device_id,
                one_time_password: self
                    .one_time_password
                    .value()
                    .unwrap_or_default()
                    .to_owned(),
                show_one_time_password: self.one_time_password.generated().is_some(),
                prompt: false,
                show_registration_url: true,
                c8y_url,
                root_certs: config.cloud_root_certs().await?,
                cert_path: cert_path.clone(),
                key,
                csr_path: instance_csr_path(&mapper_dir),
                generate_csr: true,
                retry_every: CA_POLL_INTERVAL,
                max_timeout: REGISTRATION_TIMEOUT,
                csr_template: csr_template(config),
                user,
                group,
                cloud: None,
            };
            let fresh = self.load_config().await?;
            cmd.execute(fresh).await?;
        }

        // Point the instance at certificate auth and its own certificate
        self.apply_instance_updates(&[
            KeyValue::new(key::AUTH_METHOD, AuthMethod::Certificate.to_string()),
            KeyValue::new(key::DEVICE_CERT_PATH, cert_path.to_string()),
            KeyValue::new(key::DEVICE_KEY_PATH, key_path.to_string()),
        ])
        .await?;
        Ok(())
    }

    /// Create a self-signed certificate and upload it using user credentials
    ///
    /// The Cumulocity user credentials are the method's declared inputs
    /// (`$C8Y_USER` / `$C8Y_PASSWORD`): collected by the wizard or taken
    /// from the environment; otherwise the upload step prompts for them.
    async fn register_self_signed(
        &self,
        config: TEdgeConfig,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        if self.custom_mapper_name().is_some() {
            return Err(anyhow!(
                "--register {} is not supported for custom-named instances; \
                 use {}, {}, or a register hook",
                method::SELF_SIGNED,
                method::CA,
                method::BASIC
            )
            .into());
        }
        let Some(cert_path) = self.builtin_cert_path(&config)? else {
            return Ok(());
        };
        if cert_path.exists() {
            self.detail(&format!(
                "certificate already present at {cert_path}, skipping"
            ));
            return Ok(());
        }
        if self.dry_run {
            self.ui.line(&format!(
                "would create a self-signed certificate and upload it to Cumulocity \
                 (requires {}/{} or an interactive prompt)",
                env::USER,
                env::PASSWORD
            ));
            return Ok(());
        }
        create_parent_dir(&cert_path).await?;

        let create_cli = TEdgeCertCli::Create {
            id: self.device_id.clone(),
            cloud: Some(CloudArg::C8y {
                profile: self.profile().cloned(),
            }),
        };
        let cmd = create_cli
            .build_command(&config)
            .await
            .map_err(|e| anyhow!(e))?;
        cmd.execute(config).await?;

        let username = input_value(&self.hook_envs, env::USER).unwrap_or_default();
        let password = input_value(&self.hook_envs, env::PASSWORD)
            .or_else(|| input_value(&self.hook_envs, env::PASSWORD_DEPRECATED))
            .unwrap_or_default();
        let upload_cli = TEdgeCertCli::Upload(UploadCertCli::C8y {
            username,
            password,
            profile: self.profile().cloned(),
        });
        // Reload so the device id can be derived from the new certificate
        let config = self.load_config().await?;
        let cmd = upload_cli
            .build_command(&config)
            .await
            .map_err(|e| anyhow!(e))?;
        cmd.execute(config).await?;
        Ok(())
    }

    /// Where this instance keeps its basic-auth state
    async fn basic_auth_target(&self, config: &TEdgeConfig) -> anyhow::Result<BasicAuthTarget> {
        if let Some(name) = self.custom_mapper_name() {
            let mapper_toml = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
            return Ok(BasicAuthTarget {
                credentials_path: instance_credentials_path(&self.mapper_dir(name)),
                configured_device_id: mapper_toml.device_id().map(str::to_owned),
                http_host: self.named_instance_c8y_url(name).await.ok(),
                persist_credentials_path: true,
            });
        }
        let c8y = self.c8y_config(config)?;
        // Store the credentials under the mapper's own directory
        // (following the custom-mapper convention) rather than the legacy
        // default of <config-dir>/credentials.toml —
        // unless the user explicitly configured c8y.credentials_path
        let configured: Utf8PathBuf = c8y.cloud_specific.credentials_path.clone().into();
        let legacy_default = self.config_dir.join("credentials.toml");
        let (credentials_path, persist_credentials_path) = if configured == legacy_default {
            let mapper_dir = self.mapper_dir(&self.cloud.mapper_dir_name());
            (instance_credentials_path(&mapper_dir), true)
        } else {
            (configured, false)
        };
        Ok(BasicAuthTarget {
            credentials_path,
            configured_device_id: c8y.device.id().ok(),
            http_host: c8y.cloud_specific.http.or_config_not_set().ok().cloned(),
            persist_credentials_path,
        })
    }

    /// Username/password (basic auth) registration.
    ///
    /// Obtains the credentials (requested from the platform, or supplied
    /// pre-registered), stores them (mode 600), then switches the instance
    /// to basic auth, points it at the credentials, and persists the
    /// device id the credentials belong to — it cannot be derived from a
    /// certificate in this mode and is required as the MQTT client id
    async fn register_basic_auth(
        &self,
        config: &TEdgeConfig,
        source: BasicAuthSource,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let target = self.basic_auth_target(config).await?;
        let path = &target.credentials_path;
        let effective_id = self
            .device_id
            .clone()
            .or_else(|| target.configured_device_id.clone());

        // the id the credentials were actually minted for
        let mut registered_device_id = None;
        if path.exists() {
            self.detail(&format!("credentials already present at {path}, skipping"));
            self.warn_credentials_mismatch(path, effective_id.as_deref());
        } else if self.dry_run {
            self.ui.line(&format!(
                "would store device credentials (basic auth) at {path}"
            ));
        } else if self.offline && source == BasicAuthSource::Requested {
            // the credentials request needs the cloud: deferred -
            // the auth switch and identity below are local configuration
            self.ui.line(
                "offline: credentials request deferred - \
                 re-run this command once the device is online",
            );
        } else {
            let (credentials, device_id) = match source {
                BasicAuthSource::Preregistered => {
                    let (username, password) = self.credential_inputs(
                        method::BASIC_PREREGISTERED,
                        "the issued device credentials",
                        env::DEVICE_USER,
                        env::DEVICE_PASSWORD,
                    )?;
                    let credentials = DeviceCredentials { username, password };
                    let device_id = self.resolve_preregistered_device_id(&credentials)?;
                    match (&target.http_host, self.offline) {
                        (_, true) => self.detail("offline: skipping the credentials verification"),
                        (Some(host), false) => {
                            self.verify_device_credentials(config, host, &credentials)
                                .await?
                        }
                        (None, false) => {}
                    }
                    (credentials, device_id)
                }
                BasicAuthSource::Requested => {
                    let device_id = effective_id
                        .clone()
                        .ok_or_else(|| missing_device_id_error(method::BASIC))?;
                    let host = target
                        .http_host
                        .as_ref()
                        .context("The Cumulocity URL is not configured; pass --url")?;
                    let credentials = self
                        .request_device_credentials(config, host, &device_id)
                        .await?;
                    (credentials, device_id)
                }
            };
            basic::store_credentials(path, &credentials).await?;
            self.ui.line(&format!("credentials stored at {path}"));
            registered_device_id = Some(device_id);
        }

        let mut updates = vec![KeyValue::new(
            key::AUTH_METHOD,
            AuthMethod::Basic.to_string(),
        )];
        if target.persist_credentials_path {
            updates.push(KeyValue::new(key::CREDENTIALS_PATH, path.to_string()));
        }
        if let Some(device_id) = registered_device_id.or_else(|| self.device_id.clone()) {
            updates.push(KeyValue::new(key::DEVICE_ID, device_id));
        }
        self.apply_instance_updates(&updates).await?;
        Ok(())
    }

    /// Verify the pre-registered credentials against the platform,
    /// so a typo fails at bootstrap time instead of surfacing as an
    /// opaque MQTT NotAuthorized at the connect step.
    /// An unreachable platform only warns - connect is the backstop;
    /// an untrusted one is an error, as every later exchange fails the same way
    async fn verify_device_credentials(
        &self,
        config: &TEdgeConfig,
        http_host: &HostPort<HTTPS_PORT>,
        credentials: &DeviceCredentials,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let client =
            basic::client(&config.cloud_root_certs().await?).map_err(anyhow::Error::new)?;
        let base_url = format!("https://{http_host}");
        let unverifiable =
            match registration::verify_device_credentials(&client, &base_url, credentials).await {
                Ok(registration::CredentialsCheck::Verified) => {
                    self.detail("verified the device credentials against the platform");
                    return Ok(());
                }
                Ok(registration::CredentialsCheck::Rejected) => {
                    return Err(anyhow!(
                        "The pre-registered device credentials were rejected by {http_host}. \
                     Check the username and password (and that the device user is enabled)"
                    )
                    .into())
                }
                Ok(registration::CredentialsCheck::UnexpectedStatus(status)) => {
                    format!("HTTP {status} from {http_host}")
                }
                Err(err) => {
                    let trust_store = self.trust_store(config).await;
                    match tls_trust_error(&err, &http_host.host().to_string(), &trust_store) {
                        Some(err) => return Err(err.into()),
                        None => err.to_string(),
                    }
                }
            };
        self.ui.line(&format!(
            "Warning: could not verify the device credentials ({unverifiable}); \
             the connect step will verify them"
        ));
        Ok(())
    }

    /// A method's username and password inputs, both required;
    /// the password buffer is zeroed on drop
    pub(super) fn credential_inputs(
        &self,
        method: &str,
        what: &str,
        user_env: &str,
        password_env: &str,
    ) -> anyhow::Result<(String, Zeroizing<String>)> {
        match (
            input_value(&self.hook_envs, user_env),
            input_value(&self.hook_envs, password_env),
        ) {
            (Some(user), Some(password)) => Ok((user, Zeroizing::new(password))),
            _ => Err(anyhow!(
                "The {method} registration method requires {what}: \
                 set the {user_env} and {password_env} environment variables"
            )),
        }
    }

    /// The MQTT client id must match the device the credentials belong to:
    /// derived from the issued username's `device_<id>` convention
    /// when no --device-id is given; a conflicting explicit id
    /// is warned about (the cloud will refuse it)
    fn resolve_preregistered_device_id(
        &self,
        credentials: &DeviceCredentials,
    ) -> anyhow::Result<String> {
        let derived = device_id_from_device_username(&credentials.username);
        let device_id = self
            .device_id
            .clone()
            .or_else(|| derived.clone())
            .ok_or_else(|| {
                anyhow!(
                    "Could not derive the device id from the username; \
                     provide it with --device-id"
                )
            })?;
        if let Some(derived) = derived {
            if derived != device_id {
                self.ui.line(&format!(
                    "Warning: the credentials belong to \"{derived}\" but the \
                     device id is \"{device_id}\"; the cloud will refuse the connection"
                ));
            }
        }
        Ok(device_id)
    }

    /// Warn when the stored basic-auth credentials belong to a different
    /// device id than the one this run will connect with.
    ///
    /// A Cumulocity device user may only connect as its own device,
    /// so the mismatch otherwise surfaces later as an opaque
    /// MQTT `NotAuthorized` at the connect step.
    /// The comparison relies on the platform's `device_<id>` username
    /// convention; credentials that do not follow it are left alone
    fn warn_credentials_mismatch(&self, credentials_path: &Utf8Path, device_id: Option<&str>) {
        let Some(device_id) = device_id else { return };
        let Some(credentials_id) = read_c8y_credentials(credentials_path)
            .ok()
            .and_then(|(username, _)| device_id_from_device_username(&username))
        else {
            return;
        };
        if credentials_id != device_id {
            self.ui.line(&format!(
                "Warning: the stored credentials belong to \"{credentials_id}\" \
                 but the device id is \"{device_id}\"; the cloud will refuse the \
                 connection - re-run with --re-register to obtain matching credentials"
            ));
        }
    }
}

/// The device id carried inside an issued Cumulocity device username
/// (`t<tenant-id>/device_<device-id>`), the platform's naming convention
fn device_id_from_device_username(username: &str) -> Option<String> {
    username
        .split('/')
        .nth(1)?
        .strip_prefix("device_")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_follows_the_platform_username_convention() {
        assert_eq!(
            device_id_from_device_username("t1234/device_demo01").as_deref(),
            Some("demo01")
        );
        assert_eq!(device_id_from_device_username("t1234/device_"), None);
        assert_eq!(device_id_from_device_username("t1234/admin"), None);
        assert_eq!(device_id_from_device_username("device_demo01"), None);
    }
}
