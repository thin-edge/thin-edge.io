//! The register step: obtain device credentials
//!
//! Either a built-in Cumulocity method (c8y-ca, self-signed, basic,
//! basic-preregistered), or the register.d hooks; the outcome is
//! verified by the presence of the credential artifacts.

use super::missing_device_id_error;
use super::BootstrapCommand;
use crate::cli::bootstrap::basic;
use crate::cli::bootstrap::basic::CredentialsCheck;
use crate::cli::bootstrap::cli::RegistrationMethod;
use crate::cli::bootstrap::hooks;
use crate::cli::bootstrap::hooks::HookContext;
use crate::cli::bootstrap::hooks::Phase;
use crate::cli::bootstrap::mapper_toml::MapperToml;
use crate::cli::bootstrap::tls::TrustStore;
use crate::cli::certificate::create_csr::Key;
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
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::Zeroizing;
use std::time::Duration;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::TEdgeConfig;

/// How often a pending c8y-ca registration is polled
const CA_POLL_INTERVAL: Duration = Duration::from_secs(30);

/// How often a pending basic-auth credentials request is polled
const CREDENTIALS_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// How long to wait for an operator to accept a pending registration
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(600);

/// Where a Cumulocity instance keeps its basic-auth state:
/// the default instance and profiles in the tedge config,
/// custom-named instances in their mapper.toml
struct BasicAuthTarget {
    credentials_path: Utf8PathBuf,
    /// The device id already configured for the instance
    configured_device_id: Option<String>,
    /// The platform's HTTP host, when resolvable from the configuration
    http_host: Option<String>,
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
    /// Obtain device credentials using the selected registration method
    pub(super) async fn register(
        &self,
        hook_ctx: &HookContext<'_>,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let config = self.load_config().await?;

        if matches!(self.register, RegistrationMethod::Hook { .. }) {
            return self.register_via_hooks(&config, hook_ctx).await;
        }

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
                self.apply_instance_updates(&[("device.id".to_owned(), device_id.clone())])
                    .await?;
            }
            return Ok(());
        }

        // The remaining methods are Cumulocity-specific (enforced at CLI level)
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
            RegistrationMethod::Hook { .. } => unreachable!("handled above"),
        }
    }

    /// Delegate registration to the register.d hooks and verify the outcome
    async fn register_via_hooks(
        &self,
        config: &TEdgeConfig,
        hook_ctx: &HookContext<'_>,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        // Method names are cloud vocabulary: make the scoping visible in the output
        if let Some(method) = &hook_ctx.register_method {
            self.detail(&format!(
                "using the {} \"{method}\" method",
                self.cloud_name()
            ));
        }
        let hooks_run = hooks::run_phase(Phase::Register, hook_ctx).await?;
        if hooks_run == 0 {
            let dirs = hooks::phase_dirs(Phase::Register, &self.plugin_paths);
            let dirs = dirs
                .iter()
                .map(|dir| dir.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if self.dry_run {
                self.ui.line(&format!(
                    "note: registration requires register hooks, \
                     but none are currently installed in: {dirs}"
                ));
                return Ok(());
            }
            if self.offline {
                self.ui.line(
                    "offline: registration deferred (no register hooks installed) - \
                     re-run this command once the device is online",
                );
                return Ok(());
            }
            return Err(anyhow!(
                "--register hook requires at least one register hook, \
                 but none were found in: {dirs}"
            )
            .into());
        }
        if self.dry_run {
            return Ok(());
        }

        // Verify the hooks produced the credential artifacts before connecting.
        // The connect step remains the end-to-end proof.
        let candidates = self.registration_artifacts(config).await?;
        if !candidates.iter().any(|path| path.exists()) {
            // Offline, hooks that self-skip are the expected outcome:
            // registration is deferred, not failed
            if self.offline {
                self.ui.line(
                    "offline: registration deferred (no register hook produced \
                     credentials) - re-run this command once the device is online",
                );
                return Ok(());
            }
            let paths = candidates
                .iter()
                .map(|path| path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(anyhow!(
                "The register hooks did not produce device credentials; \
                 expected one of: {paths}"
            )
            .into());
        }
        Ok(())
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
        if let Some(parent) = cert_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {parent}"))?;
        }
        // Resolved here rather than left to the cert download,
        // which would otherwise prompt for it on stdin:
        // no prompting once the pipeline has started
        let device_id = match &self.device_id {
            Some(device_id) => device_id.clone(),
            None => config
                .mapper_config::<C8yMapperSpecificConfig>(&self.profile().cloned())?
                .device
                .id()
                .map(|id| id.to_string())
                .map_err(|_| missing_device_id_error("c8y-ca"))?,
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
        let cert_dir = self.mapper_dir(name).join("device-certs");
        let cert_path = cert_dir.join("tedge-certificate.pem");
        let csr_path = cert_dir.join("tedge.csr");
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
            tokio::fs::create_dir_all(&cert_dir)
                .await
                .with_context(|| format!("Failed to create directory {cert_dir}"))?;
            let c8y_url = self.named_instance_c8y_url(name).await?;
            // Resolved here rather than left to the cert download,
            // which would otherwise prompt for it on stdin
            let device_id = match &self.device_id {
                Some(device_id) => device_id.clone(),
                None => MapperToml::load_or_empty(&self.mapper_config_path(name))
                    .await
                    .device_id()
                    .map(str::to_owned)
                    .ok_or_else(|| missing_device_id_error("c8y-ca"))?,
            };
            // The same key resolution as `tedge cert download c8y`:
            // the HSM-backed key when cryptoki is configured,
            // else the shared private key file
            let key = match config.device.cryptoki_config(None::<&dyn CloudConfig>)? {
                Some(cryptoki) => Key::Cryptoki(cryptoki),
                None => Key::Local(key_path.clone()),
            };
            let (user, group) = if config.mqtt.bridge.built_in {
                let system_config = config.read_system_config();
                (system_config.user, system_config.group)
            } else {
                (crate::BROKER_USER.to_owned(), crate::BROKER_USER.to_owned())
            };
            let csr_template = certificate::CsrTemplate {
                max_cn_size: 64,
                validity_period_days: config
                    .certificate
                    .validity
                    .requested_duration
                    .duration()
                    .as_secs() as u32
                    / (24 * 3600),
                organization_name: config.certificate.organization.to_string(),
                organizational_unit_name: config.certificate.organization_unit.to_string(),
            };
            let cmd = crate::cli::certificate::c8y::DownloadCertCmd {
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
                csr_path,
                generate_csr: true,
                retry_every: CA_POLL_INTERVAL,
                max_timeout: REGISTRATION_TIMEOUT,
                csr_template,
                user,
                group,
                cloud: None,
            };
            let fresh = self.load_config().await?;
            cmd.execute(fresh).await?;
        }

        // Point the instance at certificate auth and its own certificate
        self.apply_instance_updates(&[
            ("auth_method".to_owned(), "certificate".to_owned()),
            ("device.cert_path".to_owned(), cert_path.to_string()),
            ("device.key_path".to_owned(), key_path.to_string()),
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
                "--register self-signed is not supported for custom-named instances; \
                 use c8y-ca, basic, or a register hook"
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
            self.ui.line(
                "would create a self-signed certificate and upload it to Cumulocity \
                 (requires C8Y_USER/C8Y_PASSWORD or an interactive prompt)",
            );
            return Ok(());
        }
        if let Some(parent) = cert_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory {parent}"))?;
        }

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

        let username = self.input_value("C8Y_USER").unwrap_or_default();
        let password = self
            .input_value("C8Y_PASSWORD")
            .or_else(|| self.input_value("C8YPASS"))
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
                credentials_path: self.mapper_dir(name).join("credentials.toml"),
                configured_device_id: mapper_toml.device_id().map(str::to_owned),
                http_host: self
                    .named_instance_c8y_url(name)
                    .await
                    .ok()
                    .map(|host| host.to_string()),
                persist_credentials_path: true,
            });
        }
        let c8y = config.mapper_config::<C8yMapperSpecificConfig>(&self.profile().cloned())?;
        // Store the credentials under the mapper's own directory
        // (following the custom-mapper convention) rather than the legacy
        // default of <config-dir>/credentials.toml —
        // unless the user explicitly configured c8y.credentials_path
        let configured: Utf8PathBuf = c8y.cloud_specific.credentials_path.clone().into();
        let legacy_default = self.config_dir.join("credentials.toml");
        let (credentials_path, persist_credentials_path) = if configured == legacy_default {
            let mapper_dir = match self.profile() {
                None => "c8y".to_owned(),
                Some(profile) => format!("c8y.{profile}"),
            };
            (self.mapper_dir(&mapper_dir).join("credentials.toml"), true)
        } else {
            (configured, false)
        };
        Ok(BasicAuthTarget {
            credentials_path,
            configured_device_id: c8y.device.id().ok().map(|id| id.to_string()),
            http_host: c8y
                .cloud_specific
                .http
                .or_config_not_set()
                .ok()
                .map(|host| host.to_string()),
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
            self.warn_credentials_mismatch(path, effective_id.as_deref())
                .await;
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
                    let credentials = self.preregistered_credentials()?;
                    let device_id = self.resolve_preregistered_device_id(&credentials)?;
                    match (&target.http_host, self.offline) {
                        (_, true) => self.detail("offline: skipping the credentials verification"),
                        (Some(host), false) => {
                            let http_config = config.cloud_root_certs().await?;
                            self.verify_device_credentials(
                                host,
                                &credentials,
                                &http_config,
                                &self.trust_store(config),
                            )
                            .await?;
                        }
                        (None, false) => {}
                    }
                    (credentials, device_id)
                }
                BasicAuthSource::Requested => {
                    let device_id = effective_id
                        .clone()
                        .ok_or_else(|| missing_device_id_error("basic"))?;
                    let host = target
                        .http_host
                        .as_deref()
                        .context("The Cumulocity URL is not configured; pass --url")?;
                    let (bootstrap_user, bootstrap_password) = self.bootstrap_credentials()?;
                    let credentials = basic::request_device_credentials(
                        &format!("https://{host}"),
                        &device_id,
                        &bootstrap_user,
                        &bootstrap_password,
                        &config.cloud_root_certs().await?,
                        &self.trust_store(config),
                        CREDENTIALS_POLL_INTERVAL,
                        REGISTRATION_TIMEOUT,
                    )
                    .await?;
                    (credentials, device_id)
                }
            };
            basic::store_credentials(path, &credentials).await?;
            self.ui.line(&format!("credentials stored at {path}"));
            registered_device_id = Some(device_id);
        }

        let mut updates = vec![("auth_method".to_owned(), "basic".to_owned())];
        if target.persist_credentials_path {
            updates.push(("credentials_path".to_owned(), path.to_string()));
        }
        if let Some(device_id) = registered_device_id.or_else(|| self.device_id.clone()) {
            updates.push(("device.id".to_owned(), device_id));
        }
        self.apply_instance_updates(&updates).await?;
        Ok(())
    }

    /// Verify the pre-registered credentials against the platform,
    /// so a typo fails at bootstrap time instead of surfacing as an
    /// opaque MQTT NotAuthorized at the connect step.
    /// An unreachable platform only warns - connect is the backstop
    async fn verify_device_credentials(
        &self,
        http_host: &str,
        credentials: &basic::DeviceCredentials,
        http_config: &certificate::CloudHttpConfig,
        trust_store: &TrustStore,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        let check = basic::verify_device_credentials(
            &format!("https://{http_host}"),
            credentials,
            http_config,
            trust_store,
        )
        .await?;
        match check {
            CredentialsCheck::Verified => {
                self.detail("verified the device credentials against the platform");
                Ok(())
            }
            CredentialsCheck::Rejected => Err(anyhow!(
                "The pre-registered device credentials were rejected by {http_host}. \
                 Check the username and password (and that the device user is enabled)"
            )
            .into()),
            CredentialsCheck::Unverifiable(reason) => {
                self.ui.line(&format!(
                    "Warning: could not verify the device credentials ({reason}); \
                     the connect step will verify them"
                ));
                Ok(())
            }
        }
    }

    /// The value of a registration-method input:
    /// collected by the wizard or applied as a declared default (hook_envs),
    /// else taken from the process environment
    fn input_value(&self, env: &str) -> Option<String> {
        self.hook_envs
            .iter()
            .find(|(name, _)| name == env)
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var(env).ok().filter(|value| !value.is_empty()))
    }

    /// The pre-registered device credentials,
    /// the `basic-preregistered` method's declared inputs
    fn preregistered_credentials(&self) -> anyhow::Result<basic::DeviceCredentials> {
        match (
            self.input_value("C8Y_DEVICE_USER"),
            self.input_value("C8Y_DEVICE_PASSWORD"),
        ) {
            (Some(username), Some(password)) => Ok(basic::DeviceCredentials {
                username,
                password: Zeroizing::new(password),
            }),
            _ => Err(anyhow!(
                "The basic-preregistered method requires the issued device \
                 credentials: set the C8Y_DEVICE_USER and C8Y_DEVICE_PASSWORD \
                 environment variables"
            )),
        }
    }

    /// The tenant's device bootstrap credentials, the `basic` method's
    /// declared inputs; no default values live in the code -
    /// the bootstrap user default is descriptor metadata.
    /// The password buffer is zeroed on drop
    fn bootstrap_credentials(&self) -> anyhow::Result<(String, Zeroizing<String>)> {
        match (
            self.input_value("C8Y_BOOTSTRAP_USER"),
            self.input_value("C8Y_BOOTSTRAP_PASSWORD"),
        ) {
            (Some(user), Some(password)) => Ok((user, Zeroizing::new(password))),
            _ => Err(anyhow!(
                "The basic registration method requires the tenant's bootstrap \
                 credentials: set the C8Y_BOOTSTRAP_USER and C8Y_BOOTSTRAP_PASSWORD \
                 environment variables"
            )),
        }
    }

    /// The MQTT client id must match the device the credentials belong to:
    /// derived from the issued username's `device_<id>` convention
    /// when no --device-id is given; a conflicting explicit id
    /// is warned about (the cloud will refuse it)
    fn resolve_preregistered_device_id(
        &self,
        credentials: &basic::DeviceCredentials,
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
    async fn warn_credentials_mismatch(
        &self,
        credentials_path: &Utf8Path,
        device_id: Option<&str>,
    ) {
        let Some(device_id) = device_id else { return };
        let Some(credentials_id) = basic::read_stored_username(credentials_path)
            .await
            .as_deref()
            .and_then(device_id_from_device_username)
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
