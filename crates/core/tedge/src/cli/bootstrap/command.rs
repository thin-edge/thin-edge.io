//! The bootstrap pipeline
//!
//! `prepare → configure → register → connect → finalize`:
//! each step is a transition establishing a durable condition of the
//! device, skipped when that condition already holds —
//! which is what makes re-runs and resumed runs safe.
//!
//! The steps live in their own submodules, as does the unwind
//! (`--re-register` / `--clean`); this module holds the pipeline itself
//! and the helpers the steps share.
//! The Cumulocity-specific parts of the steps live in [`super::c8y`].

pub(super) mod configure;
mod connect;
mod register;
#[cfg(test)]
pub(super) mod test_support;
mod unwind;

use super::c8y;
use super::hooks;
use super::hooks::HookContext;
use super::hooks::Phase;
use super::invocation::Invocation;
use super::mapper_toml::instance_cert_path;
use super::mapper_toml::mapper_dir;
use super::mapper_toml::write_mapper_config;
use super::mapper_toml::MapperToml;
use super::resolve::RegistrationMethod;
use super::settings::key;
use super::settings::KeyValue;
use super::tls::TrustStore;
use super::ui::Ui;
use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::tedge_toml::DEFAULT_ROOT_CERT_PATH;
use tedge_config::TEdgeConfig;
use tedge_system_services::SystemServiceManager;

pub struct BootstrapCommand {
    pub config_dir: Utf8PathBuf,
    /// The layered hook and descriptor roots (`bootstrap.plugin_paths`),
    /// earlier entries taking precedence per file name
    pub plugin_paths: Vec<Utf8PathBuf>,
    pub service_manager: Arc<dyn SystemServiceManager>,
    pub cloud: Cloud,
    /// The declared cloud type of a custom-named instance (e.g. "c8y"),
    /// persisted as cloud_type in its mapper.toml
    pub cloud_type: Option<String>,
    pub url: Option<String>,
    pub register: RegistrationMethod,
    pub device_id: Option<String>,
    pub one_time_password: OneTimePassword,
    /// The `--set` values (full keys) and the device-global settings
    /// the cloud descriptor pins
    pub settings: Vec<KeyValue>,
    /// Config values implied by the cloud and the chosen registration method
    /// (declared by the cloud descriptor; keys relative to the instance).
    /// Applied before `settings`, so explicit `--set` values win
    pub method_settings: Vec<KeyValue>,
    /// How long to keep retrying a custom mapper's connection check;
    /// `None` means a single attempt (--no-wait)
    pub connect_timeout: Option<Duration>,
    /// Extra environment variables for hook processes
    /// (registration inputs collected by the interactive wizard,
    /// or applied as declared defaults)
    pub hook_envs: Vec<(String, String)>,
    /// Remove the instance's registration artifacts before bootstrapping,
    /// so registration re-runs against the kept configuration (--re-register)
    pub re_register: bool,
    /// Unwind the instance completely before bootstrapping:
    /// the registration artifacts plus its own configuration (--clean)
    pub clean: bool,
    /// Offline provisioning (--offline): stop the state machine at
    /// *configured* (+ staged services), deliberately and successfully;
    /// registration and connection verification are deferred
    /// to a later online run of the same command
    pub offline: bool,
    /// The effective invocation, saved as a declarative file by --save
    pub invocation: Invocation,
    /// Structured, phase-grouped console output;
    /// shared by all the runs of a sequence, which share one log file
    pub ui: Arc<Ui>,
    pub dry_run: bool,
}

/// The c8y-ca one-time password of a run
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OneTimePassword {
    /// Not a c8y-ca run, or an offline run deferring registration
    /// (a printed registration URL's password would not survive
    /// to the online run that actually registers)
    None,
    /// Supplied by the user: kept secret, neither displayed nor put in a URL
    Supplied(String),
    /// Generated upfront by this run, so the registration URL is known
    /// before the register step and can be exposed to hooks
    /// (QR codes, operator displays); displayed like `tedge cert download c8y` does
    Generated(String),
}

impl OneTimePassword {
    pub fn value(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Supplied(password) | Self::Generated(password) => Some(password),
        }
    }

    pub fn generated(&self) -> Option<&str> {
        match self {
            Self::Generated(password) => Some(password),
            _ => None,
        }
    }
}

/// One or several bootstrap runs (from the flags, or a --from file),
/// executed in order; a failing run stops the sequence
pub struct BootstrapSequence {
    pub commands: Vec<BootstrapCommand>,
    /// Save the effective invocations as a declarative file (--save)
    pub save_path: Option<Utf8PathBuf>,
}

#[async_trait::async_trait]
impl Command for BootstrapSequence {
    fn description(&self) -> String {
        match self.commands.as_slice() {
            [command] => command.description(),
            commands => format!("bootstrap {} cloud instances", commands.len()),
        }
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        // The declarative capture is written upfront, deliberately even on
        // dry runs: walk the wizard, save the answers, apply nothing
        if let Some(path) = &self.save_path {
            let invocations: Vec<Invocation> = self
                .commands
                .iter()
                .map(|command| command.invocation.clone())
                .collect();
            super::invocation::save_invocations(path, &invocations).await?;
            let what = match invocations.len() {
                1 => "the bootstrap invocation".to_owned(),
                n => format!("{n} bootstrap invocations"),
            };
            eprintln!("Saved {what} to {path} (replay with: tedge bootstrap --from {path})\n");
        }
        let mut config = Some(config);
        for (i, command) in self.commands.iter().enumerate() {
            if i > 0 {
                eprintln!();
            }
            // Each run needs a fresh snapshot of what the previous one wrote
            let current = match config.take() {
                Some(config) => config,
                None => command.load_config().await?,
            };
            command.execute(current).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl Command for BootstrapCommand {
    fn description(&self) -> String {
        format!("bootstrap the device to {}", self.cloud)
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        let result = self.run(config).await;
        self.ui.finish(result.is_ok(), &self.summary().await);
        result
    }
}

impl BootstrapCommand {
    async fn run(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.ui
            .begin(&format!("Bootstrapping the device to {}", self.cloud));
        if self.dry_run {
            self.ui.line("(dry-run: no changes will be made)");
        }
        let mut hook_envs = self.hook_envs.clone();
        if let Some(device_id) = &self.device_id {
            // The config env override for device.id, so `tedge config get
            // device.id` inside a hook resolves to the bootstrapped
            // identity even before it is persisted
            hook_envs.push((hooks::env::DEVICE_ID.to_owned(), device_id.clone()));
        }
        let mut hook_ctx = HookContext {
            config_dir: &self.config_dir,
            plugin_paths: &self.plugin_paths,
            cloud: self.cloud_name(),
            url: self.url.clone(),
            cloud_type: self.cloud_type.clone(),
            profile: self.profile().map(|p| p.to_string()),
            register_method: match &self.register {
                RegistrationMethod::Hook { method } => method.clone(),
                _ => None,
            },
            envs: hook_envs,
            re_register: self.re_register,
            clean: self.clean,
            offline: self.offline,
            ui: self.ui.as_ref(),
            dry_run: self.dry_run,
        };

        if self.re_register || self.clean {
            self.ui.phase("cleaned");
            // Artifacts first (their paths come from the configuration
            // that --clean is about to unwind)
            self.remove_registration_artifacts(&config).await?;
        }
        let config = if self.clean {
            self.unwind_instance_config(config).await?
        } else {
            config
        };

        self.ui.phase("prepared");
        let prepare_hooks = hooks::run_phase(Phase::Prepare, &hook_ctx).await?;
        // Prepare hooks may write config (e.g. installing server trust,
        // resolving the endpoints, or generating the device id),
        // so the configure step must not use the snapshot taken
        // before they ran
        let config = if prepare_hooks > 0 && !self.dry_run {
            self.load_config().await?
        } else {
            config
        };

        // Expose the pending registration to configure-phase and later
        // hooks (QR codes, operator displays, vendor UIs).
        // Computed after the prepare phase, so a device id *generated*
        // by a prepare hook (e.g. from a serial number) is included.
        // The URL carries the one-time password, so it travels via the
        // environment, never argv (and not as TEDGE_*, which is the
        // config-override namespace)
        if let Some(url) = self.pending_registration_url(&config).await {
            hook_ctx
                .envs
                .push((c8y::env::REGISTRATION_URL.to_owned(), url));
        }

        self.ui.phase("configured");
        self.configure(config).await?;
        hooks::run_phase(Phase::Configure, &hook_ctx).await?;

        // Deferred steps are named as such on the checklist
        // instead of ticking off a state that was not reached
        let register_label = match (&self.register, self.offline) {
            (_, false) => "registered",
            (RegistrationMethod::Hook { .. }, true) => "register hooks run (offline)",
            // no cloud exchange: pre-registered credentials store offline too
            (RegistrationMethod::BasicPreregistered, true) => "registered",
            (_, true) => "registration deferred",
        };
        self.ui.phase(register_label);
        self.register(&hook_ctx).await?;

        // services started and enabled, cloud checks skipped -
        // the semantics of `tedge connect --offline`;
        // when staging itself is blocked, the label says deferred
        let staging_blocker = match self.offline {
            true => self.offline_staging_blocker().await,
            false => None,
        };
        self.ui.phase(match (self.offline, staging_blocker) {
            (false, _) => "connected",
            (true, None) => "connection staged",
            (true, Some(_)) => "connection deferred",
        });
        self.connect(staging_blocker).await?;

        if self.offline {
            // finalize hooks mean "bootstrapped *and* connected": deferred
            self.ui.debug("offline: finalize hooks deferred");
        } else {
            self.ui.phase("finalized");
            hooks::run_phase(Phase::Finalize, &hook_ctx).await?;
        }

        Ok(())
    }

    /// The key facts for the final summary card,
    /// read back from the final configuration where possible
    /// (so a re-run without --device-id still reports the identity)
    async fn summary(&self) -> Vec<(&'static str, String)> {
        let mut rows: Vec<(&'static str, String)> = Vec::new();
        let config = self.load_config().await.ok();
        let device_id = self.device_id.clone().or_else(|| {
            config
                .as_ref()
                .and_then(|config| read_config_string(config, key::DEVICE_ID))
        });
        if let Some(device_id) = device_id {
            rows.push(("device id", device_id));
        }
        rows.push(("cloud", self.cloud.to_string()));
        if let Some(register) = &self.invocation.register {
            rows.push(("register", register.clone()));
        }
        let url = match (&self.invocation.url, &config) {
            (Some(url), _) => Some(url.clone()),
            (None, Some(config)) => super::resolve::configured_url(config, &self.cloud).await,
            (None, None) => None,
        };
        if let Some(url) = url {
            // a configured c8y.http endpoint renders with its port
            rows.push(("url", url.trim_end_matches(":443").to_owned()));
        }
        if self.offline {
            let registered = match &config {
                Some(config) => self.registration_present(config).await,
                None => false,
            };
            let deferred = match registered {
                // services are staged; only the verification awaits network
                true => "connection verification",
                false => "registration, connection verification",
            };
            rows.push(("deferred", deferred.to_owned()));
        }
        rows
    }

    /// Detail lines: logged only on real runs,
    /// always shown on a dry run, where they are the whole point
    pub(super) fn detail(&self, message: &str) {
        if self.dry_run {
            self.ui.line(message);
        } else {
            self.ui.debug(message);
        }
    }

    /// Report a step's config updates (what a dry run would set);
    /// secret-looking values are masked on the console and in the log
    pub(super) fn report_updates(&self, prefix: &str, updates: &[KeyValue]) {
        let verb = if self.dry_run { "would set" } else { "set" };
        for KeyValue { key, value } in updates {
            self.detail(&format!(
                "{verb} {prefix}{key}={}",
                display_value(key, value)
            ));
        }
    }

    /// Report and apply instance-scoped updates where the instance keeps
    /// its configuration: a custom mapper's mapper.toml, or the cloud's
    /// (profile-qualified) tedge config keys.
    /// `device.id` is the exception for built-in clouds: it is the
    /// device-global identity, not an instance key
    pub(super) async fn apply_instance_updates(&self, updates: &[KeyValue]) -> anyhow::Result<()> {
        match self.custom_mapper_name() {
            Some(name) => {
                self.report_updates(&format!("{name}."), updates);
                if !self.dry_run {
                    write_mapper_config(&self.mapper_config_path(name), updates).await?;
                }
            }
            None => {
                let updates: Vec<KeyValue> = updates
                    .iter()
                    .map(|update| match update.key.as_str() {
                        key::DEVICE_ID => update.clone(),
                        setting => self.instance_setting(setting, update.value.clone()),
                    })
                    .collect();
                self.report_updates("", &updates);
                if !self.dry_run {
                    apply_tedge_config_updates(self.load_config().await?, &updates).await?;
                }
            }
        }
        Ok(())
    }

    /// Whether registration artifacts from a previous run are present,
    /// i.e. the registration step will keep them instead of obtaining
    /// new credentials
    pub(super) async fn registration_present(&self, config: &TEdgeConfig) -> bool {
        match self.registration_artifacts(config).await {
            Ok(paths) => paths.iter().any(|path| path.exists()),
            // e.g. a not-yet-created profile: nothing exists yet
            Err(_) => false,
        }
    }

    /// The credential artifacts that prove registration happened,
    /// depending on the cloud and its auth configuration
    pub(super) async fn registration_artifacts(
        &self,
        config: &TEdgeConfig,
    ) -> anyhow::Result<Vec<Utf8PathBuf>> {
        match self.custom_mapper_name() {
            None => {
                // Built-in clouds: a device certificate
                // (or, for c8y basic auth, the credentials file)
                let cert_path: Utf8PathBuf = config
                    .device_cert_path(Some(&self.cloud))
                    .map_err(anyhow::Error::new)?
                    .into();
                let mut candidates = vec![cert_path];
                if self.is_c8y() {
                    let c8y_config = self.c8y_config(config)?;
                    candidates.push(c8y_config.cloud_specific.credentials_path.clone().into());
                }
                Ok(candidates)
            }
            Some(name) => Ok(self.custom_mapper_artifacts(name).await),
        }
    }

    /// A custom mapper's *own* registration artifacts: its credentials file,
    /// its per-instance certificate, and the certificate its mapper.toml
    /// points at (a register hook reusing the shared device certificate
    /// declares that via `device.cert_path`).
    /// The shared certificate never counts implicitly:
    /// it says nothing about this instance's registration
    /// (nor about a token-authenticated cloud)
    async fn custom_mapper_artifacts(&self, name: &str) -> Vec<Utf8PathBuf> {
        let mapper_toml = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
        let mut artifacts = vec![
            mapper_toml.credentials_path(),
            instance_cert_path(&self.mapper_dir(name)),
        ];
        if let Some(cert_path) = mapper_toml.get_str(key::DEVICE_CERT_PATH) {
            artifacts.push(Utf8PathBuf::from(cert_path));
        }
        artifacts
    }
}

impl BootstrapCommand {
    pub(super) fn profile(&self) -> Option<&ProfileName> {
        self.cloud.profile_name()
    }

    /// The short cloud name, as passed to hooks and used as a config key prefix
    pub(super) fn cloud_name(&self) -> &str {
        self.cloud.short_name()
    }

    pub(super) fn custom_mapper_name(&self) -> Option<&str> {
        match &self.cloud {
            Cloud::Custom(name) => Some(name),
            _ => None,
        }
    }

    pub(super) fn mapper_dir(&self, name: &str) -> Utf8PathBuf {
        mapper_dir(&self.config_dir, name)
    }

    pub(super) fn mapper_config_path(&self, name: &str) -> Utf8PathBuf {
        MapperToml::path_for(&self.config_dir, name)
    }

    /// The instance-scoped config key of a setting:
    /// `c8y.<setting>`, `c8y.profiles.<p>.<setting>`, or `<mapper>.<setting>`
    pub(super) fn instance_key(&self, setting: &str) -> String {
        format!("{}.{setting}", self.cloud.config_prefix())
    }

    /// An instance-scoped setting, as a config update
    pub(super) fn instance_setting(&self, setting: &str, value: impl Into<String>) -> KeyValue {
        KeyValue::new(self.instance_key(setting), value)
    }

    /// Whether the user gave an instance-scoped setting with `--set`
    pub(super) fn user_set(&self, setting: &str) -> bool {
        let key = self.instance_key(setting);
        self.settings.iter().any(|s| s.key == key)
    }

    /// A non-empty instance-scoped setting of a built-in cloud,
    /// read from the tedge config
    pub(super) fn read_instance_setting(
        &self,
        config: &TEdgeConfig,
        setting: &str,
    ) -> Option<String> {
        read_config_string(config, &self.instance_key(setting))
    }

    /// The trust store this cloud instance verifies the platform against:
    /// the config key and the path in effect
    ///
    /// The MQTT bridge and the HTTP proxy use the same store,
    /// so a failure against it is worth reporting in these terms
    pub(super) async fn trust_store(&self, config: &TEdgeConfig) -> TrustStore {
        let (key, path) = match self.custom_mapper_name() {
            Some(name) => (
                format!("{name}.{}", key::DEVICE_ROOT_CERT_PATH),
                MapperToml::load_or_empty(&self.mapper_config_path(name))
                    .await
                    .get_str(key::DEVICE_ROOT_CERT_PATH)
                    .map(str::to_owned),
            ),
            None => (
                self.instance_key(key::ROOT_CERT_PATH),
                self.read_instance_setting(config, key::ROOT_CERT_PATH),
            ),
        };
        TrustStore {
            key,
            path: path.unwrap_or_else(|| DEFAULT_ROOT_CERT_PATH.to_owned()),
        }
    }

    /// Re-load the configuration from disk.
    ///
    /// The `TEdgeConfig` passed around is an immutable snapshot,
    /// so it must be re-loaded after each step that updates the config.
    pub(super) async fn load_config(&self) -> anyhow::Result<TEdgeConfig> {
        TEdgeConfig::load(&self.config_dir)
            .await
            .context("Failed to reload the tedge configuration")
    }
}

/// A non-empty string value of the tedge config, `None` for an unknown or unset key
pub(super) fn read_config_string(config: &TEdgeConfig, key: &str) -> Option<String> {
    let key = key.parse::<ReadableKey>().ok()?;
    config
        .read_string(&key)
        .ok()
        .filter(|value| !value.is_empty())
}

/// A registration method needs a device identity and none could be found
pub(super) fn missing_device_id_error(method: &str) -> anyhow::Error {
    anyhow!(
        "A device id is required for {method} registration; \
         provide one with --device-id (or {})",
        hooks::env::DEVICE_ID
    )
}

/// Config values are echoed to the console and the log:
/// anything that looks like a secret is masked
fn display_value(key: &str, value: &str) -> String {
    let key = key.to_ascii_lowercase();
    let last = key.rsplit('.').next().unwrap_or(&key);
    let secret = ["password", "secret", "token"]
        .iter()
        .any(|word| key.contains(word))
        || last == "pin";
    if secret && !value.is_empty() {
        "********".to_owned()
    } else {
        value.to_owned()
    }
}

/// Apply updates to the tedge config, validating each key
async fn apply_tedge_config_updates(
    config: TEdgeConfig,
    updates: &[KeyValue],
) -> anyhow::Result<()> {
    let updates: Vec<(WritableKey, &str)> = updates
        .iter()
        .map(|update| {
            let key = update
                .key
                .parse::<WritableKey>()
                .map_err(|e| anyhow!("Invalid config key {:?}: {e}", update.key))?;
            Ok((key, update.value.as_str()))
        })
        .collect::<anyhow::Result<_>>()?;
    config
        .update_toml(&|dto, _reader| {
            for (key, value) in &updates {
                dto.try_update_str(key, value)?;
            }
            Ok(())
        })
        .await
        .map_err(anyhow::Error::new)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::test_support::touch;
    use super::test_support::Fixture;
    use super::*;

    #[test]
    fn secret_looking_values_are_masked() {
        assert_eq!(display_value("proxy.password", "hunter2"), "********");
        assert_eq!(display_value("acme.api_token", "abc"), "********");
        assert_eq!(display_value("device.cryptoki.pin", "1234"), "********");
        assert_eq!(display_value("c8y.url", "example.com"), "example.com");
        // an unset secret is shown as such
        assert_eq!(display_value("proxy.password", ""), "");
    }

    #[tokio::test]
    async fn a_custom_mapper_is_registered_by_its_own_artifacts_only() {
        let fx = Fixture::new("").await;
        let command = fx.command(Cloud::Custom("acme".into()));
        let config = fx.config().await;
        assert!(!command.registration_present(&config).await);

        // the shared device certificate says nothing about the instance
        let shared_cert: Utf8PathBuf = config.device_cert_path(None::<&Cloud>).unwrap().into();
        touch(&shared_cert);
        assert!(!command.registration_present(&config).await);

        // unless the instance's mapper.toml declares it reuses it
        let path = MapperToml::path_for(&fx.config_dir, "acme");
        write_mapper_config(
            &path,
            &[KeyValue::new(
                key::DEVICE_CERT_PATH,
                shared_cert.to_string(),
            )],
        )
        .await
        .unwrap();
        assert!(command.registration_present(&config).await);

        // its credentials file counts, at the configured (relative) path
        write_mapper_config(
            &path,
            &[
                KeyValue::new(key::DEVICE_CERT_PATH, ""),
                KeyValue::new(key::CREDENTIALS_PATH, "secrets/acme.toml"),
            ],
        )
        .await
        .unwrap();
        assert!(!command.registration_present(&config).await);
        touch(&fx.config_dir.join("mappers/acme/secrets/acme.toml"));
        assert!(command.registration_present(&config).await);
    }

    #[tokio::test]
    async fn instance_updates_land_where_the_instance_keeps_its_config() {
        let fx = Fixture::new("").await;
        let updates = [
            KeyValue::new(key::AUTH_METHOD, "basic"),
            KeyValue::new(key::DEVICE_ID, "demo01"),
        ];

        // a built-in cloud: its config keys, the device id being global
        let command = fx.command(Cloud::c8y(None));
        command.apply_instance_updates(&updates).await.unwrap();
        let config = fx.config().await;
        assert_eq!(
            read_config_string(&config, "c8y.auth_method").as_deref(),
            Some("basic")
        );
        assert_eq!(
            read_config_string(&config, "device.id").as_deref(),
            Some("demo01")
        );

        // a custom mapper: its mapper.toml
        let command = fx.command(Cloud::Custom("acme".into()));
        command.apply_instance_updates(&updates).await.unwrap();
        let mapper_toml = MapperToml::load(&MapperToml::path_for(&fx.config_dir, "acme"))
            .await
            .unwrap();
        assert_eq!(mapper_toml.get_str(key::AUTH_METHOD), Some("basic"));
        assert_eq!(mapper_toml.device_id(), Some("demo01"));

        // a dry run changes nothing
        let mut command = fx.command(Cloud::Custom("dry".into()));
        command.dry_run = true;
        command.apply_instance_updates(&updates).await.unwrap();
        assert!(!MapperToml::path_for(&fx.config_dir, "dry").exists());
    }
}
