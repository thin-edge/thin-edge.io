//! The bootstrap pipeline
//!
//! `prepare → configure → register → connect → finalize`:
//! each step is a transition establishing a durable condition of the
//! device, skipped when that condition already holds —
//! which is what makes re-runs and resumed runs safe.
//!
//! The configure, register and connect steps live in their own
//! submodules; this module holds the pipeline itself, the unwind
//! (`--re-register` / `--clean`), and the helpers the steps share.

mod configure;
mod connect;
mod register;

use super::cli::KeyValue;
use super::cli::RegistrationMethod;
use super::hooks;
use super::hooks::HookContext;
use super::hooks::Phase;
use super::invocation::Invocation;
use super::mapper_toml::write_mapper_config;
use super::mapper_toml::MapperToml;
use super::ui::Ui;
use crate::cli::common::Cloud;
use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::models::HostPort;
use tedge_config::models::HTTPS_PORT;
use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;
use tedge_system_services::SystemService;
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
    pub settings: Vec<KeyValue>,
    /// Config values implied by the cloud and the chosen registration method
    /// (declared by the cloud descriptor; keys relative to the cloud).
    /// Applied before `settings`, so explicit `--set` values win
    pub method_settings: Vec<(String, String)>,
    /// How long to keep retrying a custom mapper's connection check;
    /// `None` means a single attempt (--no-wait)
    pub connect_timeout: Option<Duration>,
    /// Extra environment variables for hook processes
    /// (registration inputs collected by the interactive wizard)
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
    /// Save the effective invocation as a declarative file (--save)
    pub save_path: Option<Utf8PathBuf>,
    /// Structured, phase-grouped console output (--verbose widens it);
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

/// The instance-scoped keys bootstrap writes for a built-in cloud,
/// relative to the instance (`c8y.` or `c8y.profiles.<p>.`):
/// what `--clean` unwinds, leaving the rest of the cloud's section alone
const BUILTIN_MANAGED_KEYS: &[&str] = &[
    "url",
    "http",
    "mqtt",
    "auth_method",
    "credentials_path",
    "bridge.topic_prefix",
    "proxy.bind.port",
    "device.cert_path",
    "device.csr_path",
];

/// The keys bootstrap writes into a custom mapper's `mapper.toml`
const MAPPER_MANAGED_KEYS: &[&str] = &[
    "url",
    "device.id",
    "auth_method",
    "credentials_path",
    "cloud_type",
    "bridge.topic_prefix",
    "proxy.bind.port",
    "device.cert_path",
    "device.key_path",
];

/// Several bootstrap runs (from a --from file), executed in order;
/// a failing run stops the sequence
pub struct BootstrapSequence {
    pub commands: Vec<BootstrapCommand>,
    /// Save the effective invocations as a declarative file (--save)
    pub save_path: Option<Utf8PathBuf>,
}

#[async_trait::async_trait]
impl Command for BootstrapSequence {
    fn description(&self) -> String {
        format!("bootstrap {} cloud instances", self.commands.len())
    }

    async fn execute(&self, config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        if let Some(path) = &self.save_path {
            let invocations: Vec<Invocation> = self
                .commands
                .iter()
                .map(|command| command.invocation.clone())
                .collect();
            super::invocation::save_invocations(path, &invocations).await?;
            eprintln!(
                "Saved {} bootstrap invocations to {path} (replay with: tedge bootstrap --from {path})\n",
                invocations.len()
            );
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
        // The declarative capture is written upfront, deliberately even on
        // dry runs: walk the wizard, save the answers, apply nothing
        if let Some(path) = &self.save_path {
            super::invocation::save_invocations(path, std::slice::from_ref(&self.invocation))
                .await?;
            eprintln!(
                "Saved the bootstrap invocation to {path} (replay with: tedge bootstrap --from {path})\n"
            );
        }
        let result = self.run(config).await;
        let summary = self.summary().await;
        match &result {
            Ok(()) => self.ui.finish_success(&summary),
            Err(_) => self.ui.finish_failure(&summary),
        }
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
            // TEDGE_DEVICE_ID is the config env override for device.id,
            // so `tedge config get device.id` inside a hook resolves
            // to the bootstrapped identity even before it is persisted
            hook_envs.push(("TEDGE_DEVICE_ID".to_owned(), device_id.clone()));
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
            hook_ctx.envs.push(("C8Y_REGISTRATION_URL".to_owned(), url));
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
        let connect_label = match self.offline {
            false => "connected",
            true => match self.offline_staging_blocker().await {
                None => "connection staged",
                Some(_) => "connection deferred",
            },
        };
        self.ui.phase(connect_label);
        self.connect().await?;

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
            let config = config.as_ref()?;
            let key = "device.id".parse::<ReadableKey>().ok()?;
            config.read_string(&key).ok().filter(|id| !id.is_empty())
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
            (None, Some(config)) => self.c8y_registration_host(config).await,
            (None, None) => None,
        };
        if let Some(url) = url {
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

    /// Detail lines: logged (and shown with --verbose) on real runs,
    /// always shown on a dry run, where they are the whole point
    fn detail(&self, message: &str) {
        if self.dry_run {
            self.ui.line(message);
        } else {
            self.ui.debug(message);
        }
    }

    /// Report a step's config updates (what a dry run would set);
    /// secret-looking values are masked on the console and in the log
    fn report_updates<'a>(
        &self,
        prefix: &str,
        updates: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) {
        let verb = if self.dry_run { "would set" } else { "set" };
        for (key, value) in updates {
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
    async fn apply_instance_updates(&self, updates: &[(String, String)]) -> anyhow::Result<()> {
        match self.custom_mapper_name() {
            Some(name) => {
                self.report_updates(&format!("{name}."), pairs(updates));
                if !self.dry_run {
                    write_mapper_config(&self.mapper_config_path(name), updates).await?;
                }
            }
            None => {
                let updates: Vec<KeyValue> = updates
                    .iter()
                    .map(|(key, value)| match key.as_str() {
                        "device.id" => KeyValue {
                            key: key.clone(),
                            value: value.clone(),
                        },
                        setting => self.config_key(setting, value.clone()),
                    })
                    .collect();
                self.report_updates(
                    "",
                    updates
                        .iter()
                        .map(|update| (update.key.as_str(), update.value.as_str())),
                );
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
    pub(crate) async fn registration_present(&self, config: &TEdgeConfig) -> bool {
        match self.registration_artifacts(config).await {
            Ok(paths) => paths.iter().any(|path| path.exists()),
            // e.g. a not-yet-created profile: nothing exists yet
            Err(_) => false,
        }
    }

    /// The credential artifacts that prove registration happened,
    /// depending on the cloud and its auth configuration
    async fn registration_artifacts(
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
                #[cfg(feature = "c8y")]
                if let Cloud::C8y(_) = &self.cloud {
                    let c8y_config = config
                        .mapper_config::<C8yMapperSpecificConfig>(&self.profile().cloned())?;
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
            self.mapper_dir(name)
                .join("device-certs/tedge-certificate.pem"),
        ];
        if let Some(cert_path) = mapper_toml.get_str(&["device", "cert_path"]) {
            artifacts.push(Utf8PathBuf::from(cert_path));
        }
        artifacts
    }

    /// The Cumulocity device-registration URL of the registration this run
    /// is about to perform: c8y-ca, with a known device id and host,
    /// and not skipped (no certificate yet, or a --re-register/--clean run).
    /// The URL is pre-filled with the pre-generated one-time password
    async fn pending_registration_url(&self, config: &TEdgeConfig) -> Option<String> {
        if !matches!(self.register, RegistrationMethod::C8yCa) {
            return None;
        }
        // a password supplied by the user is kept secret
        let password = self.one_time_password.generated()?;
        let cert_path: Option<Utf8PathBuf> = match self.custom_mapper_name() {
            Some(name) => Some(
                self.mapper_dir(name)
                    .join("device-certs/tedge-certificate.pem"),
            ),
            None => config
                .device_cert_path(Some(&self.cloud))
                .ok()
                .map(Into::into),
        };
        if let Some(cert_path) = cert_path {
            // registration is skipped while the certificate exists
            if cert_path.exists() && !self.re_register && !self.clean {
                return None;
            }
        }
        let device_id = match &self.device_id {
            Some(device_id) => device_id.clone(),
            None => {
                let key = "device.id".parse::<ReadableKey>().ok()?;
                config.read_string(&key).ok().filter(|id| !id.is_empty())?
            }
        };
        let host = self.c8y_registration_host(config).await?;
        let host = HostPort::<HTTPS_PORT>::try_from(host.as_str()).ok()?;
        Some(crate::cli::certificate::c8y::registration_url(
            &host,
            &device_id,
            Some(password),
        ))
    }

    /// The Cumulocity HTTP host this run registers against
    async fn c8y_registration_host(&self, config: &TEdgeConfig) -> Option<String> {
        if let Some(url) = &self.url {
            return configure::url_host(&configure::normalize_http_url(url)).ok();
        }
        match self.custom_mapper_name() {
            Some(name) => Some(self.named_instance_c8y_url(name).await.ok()?.to_string()),
            None => ["http", "url"]
                .iter()
                .find_map(|setting| self.read_instance_setting(config, setting)),
        }
    }

    /// The Cumulocity URL of a custom-named c8y instance, from its mapper.toml
    async fn named_instance_c8y_url(&self, name: &str) -> anyhow::Result<HostPort<HTTPS_PORT>> {
        let path = self.mapper_config_path(name);
        let mapper_toml = MapperToml::load(&path).await?;
        let url = mapper_toml
            .url()
            .with_context(|| format!("The {name} instance has no URL configured; pass --url"))?;
        HostPort::try_from(url).map_err(|e| anyhow!("Invalid URL {url:?} in {path}: {e}"))
    }

    /// Remove the instance's registration artifacts (stopping its mapper),
    /// so the registration step runs afresh instead of skipping.
    ///
    /// Instance-scoped artifacts are removed unconditionally;
    /// the default instance's certificate is the shared device certificate,
    /// removed with a warning since other cloud connections may use it.
    async fn remove_registration_artifacts(
        &self,
        config: &TEdgeConfig,
    ) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.custom_mapper_name() {
            Some(name) => {
                let service_name = format!("tedge-mapper-{name}");
                if self.dry_run {
                    self.detail(&format!("would stop and disable {service_name}"));
                } else {
                    let service = SystemService {
                        name: &service_name,
                        profile: None,
                    };
                    // Best-effort: the service may not be installed or running
                    let _ = self.service_manager.stop_service(service).await;
                    let _ = self.service_manager.disable_service(service).await;
                }
                let mapper_toml = MapperToml::load_or_empty(&self.mapper_config_path(name)).await;
                self.remove_file(&mapper_toml.credentials_path()).await?;
                self.remove_dir(&self.mapper_dir(name).join("device-certs"))
                    .await?;
            }
            None => {
                let cert_path: Utf8PathBuf = config
                    .device_cert_path(Some(&self.cloud))
                    .map_err(anyhow::Error::new)?
                    .into();
                let global_cert: Utf8PathBuf = config
                    .device_cert_path(None::<&Cloud>)
                    .map_err(anyhow::Error::new)?
                    .into();
                let shared = cert_path == global_cert;
                if shared && cert_path.exists() {
                    self.warn_shared_certificate(config, &cert_path).await;
                }
                self.remove_file(&cert_path).await?;
                if let Some(csr) = self.read_instance_setting(config, "device.csr_path") {
                    self.remove_file(Utf8Path::new(&csr)).await?;
                }
                #[cfg(feature = "c8y")]
                if let Cloud::C8y(_) = &self.cloud {
                    let c8y_config = config
                        .mapper_config::<C8yMapperSpecificConfig>(&self.profile().cloned())?;
                    let credentials: Utf8PathBuf =
                        c8y_config.cloud_specific.credentials_path.clone().into();
                    self.remove_file(&credentials).await?;
                }
                // The private key can be recreated for the default instance,
                // but is shared *by* named instances and profiles —
                // remove it only together with the shared certificate
                if shared {
                    let key_path: Utf8PathBuf = config
                        .device_key_path(Some(&self.cloud))
                        .map_err(anyhow::Error::new)?
                        .into();
                    self.remove_file(&key_path).await?;
                }
            }
        }
        Ok(())
    }

    /// Unwind the instance's own configuration (`--clean`):
    /// the keys bootstrap manages in the cloud's tedge config section,
    /// or in a custom mapper's mapper.toml — plus the settings this run
    /// applies (descriptor-implied and --set values scoped to the instance).
    ///
    /// Anything else is not bootstrap's to remove:
    /// the rest of the cloud's section (software management, SmartREST
    /// templates, proxies, ...), device-global settings (proxy.address,
    /// the shared device.id), and package-shipped mapper content
    /// (removing `mappers/<name>/` outright belongs to a future
    /// `tedge mapper remove`).
    ///
    /// Returns a configuration snapshot reflecting the unwind.
    async fn unwind_instance_config(&self, config: TEdgeConfig) -> anyhow::Result<TEdgeConfig> {
        // The managed keys, the settings implied by the descriptor, and the
        // --set values — all as full keys (the latter already are)
        let managed = |managed: &'static [&'static str], prefix: String| {
            managed
                .iter()
                .map(|key| (*key).to_owned())
                .chain(self.method_settings.iter().map(|(key, _)| key.clone()))
                .map(move |key| format!("{prefix}{key}"))
                .chain(self.settings.iter().map(|setting| setting.key.clone()))
        };
        match self.custom_mapper_name() {
            Some(name) => {
                let path = self.mapper_config_path(name);
                let prefix = format!("{name}.");
                if self.dry_run {
                    self.detail(&format!("would unset the bootstrap-managed keys in {path}"));
                    return Ok(config);
                }
                if path.exists() {
                    let mut mapper_toml = MapperToml::load(&path).await?;
                    for key in managed(MAPPER_MANAGED_KEYS, prefix.clone()) {
                        if let Some(key) = key.strip_prefix(&prefix) {
                            mapper_toml.unset(key);
                        }
                    }
                    mapper_toml.save().await?;
                    self.detail(&format!("unset the bootstrap-managed keys in {path}"));
                }
                Ok(config)
            }
            None => {
                // c8y.* for the default instance, c8y.profiles.<p>.* for a
                // profile — never another instance's keys
                let prefix = self.instance_key("");
                let profiles_prefix = format!("{}.profiles.", self.cloud_name());
                let keys: Vec<WritableKey> = managed(BUILTIN_MANAGED_KEYS, prefix.clone())
                    .filter(|key| {
                        key.starts_with(&prefix)
                            && (self.profile().is_some() || !key.starts_with(&profiles_prefix))
                    })
                    .filter_map(|key| key.parse::<WritableKey>().ok())
                    .collect();
                if self.dry_run {
                    self.detail(&format!("would unset the bootstrap-managed {prefix}* keys"));
                    return Ok(config);
                }
                config
                    .update_toml(&|dto, _reader| {
                        for key in &keys {
                            dto.try_unset_key(key)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(anyhow::Error::new)?;
                self.detail(&format!("unset the bootstrap-managed {prefix}* keys"));
                self.load_config().await
            }
        }
    }

    /// The shared certificate is about to be removed: name who else uses it
    async fn warn_shared_certificate(&self, config: &TEdgeConfig, cert_path: &Utf8Path) {
        let mut others = Vec::new();
        for cloud_key in ["c8y", "az", "aws"] {
            if cloud_key == self.cloud_name() {
                continue;
            }
            let configured = format!("{cloud_key}.url")
                .parse::<ReadableKey>()
                .ok()
                .and_then(|key| config.read_string(&key).ok())
                .is_some_and(|url| !url.is_empty());
            if configured {
                others.push(cloud_key.to_owned());
            }
        }
        let builtin_dir = |name: &str| {
            ["c8y", "az", "aws"]
                .iter()
                .any(|cloud| name == *cloud || name.starts_with(&format!("{cloud}.")))
        };
        if let Ok(mut entries) = tokio::fs::read_dir(self.config_dir.join("mappers")).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let Ok(path) = Utf8PathBuf::try_from(entry.path()) else {
                    continue;
                };
                if let Some(name) = path.file_name() {
                    if !builtin_dir(name) && path.join("mapper.toml").exists() {
                        others.push(name.to_owned());
                    }
                }
            }
        }
        let also = if others.is_empty() {
            String::new()
        } else {
            format!("; also configured on this device: {}", others.join(", "))
        };
        self.ui.line(&format!(
            "Warning: removing the shared device certificate {cert_path}{also}. \
             Other cloud connections using it will need to re-register"
        ));
    }

    async fn remove_file(&self, path: &Utf8Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        if self.dry_run {
            self.detail(&format!("would remove {path}"));
        } else {
            tokio::fs::remove_file(path)
                .await
                .with_context(|| format!("Failed to remove {path}"))?;
            self.detail(&format!("removed {path}"));
        }
        Ok(())
    }

    async fn remove_dir(&self, path: &Utf8Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        if self.dry_run {
            self.detail(&format!("would remove {path}"));
        } else {
            tokio::fs::remove_dir_all(path)
                .await
                .with_context(|| format!("Failed to remove {path}"))?;
            self.detail(&format!("removed {path}"));
        }
        Ok(())
    }
}

impl BootstrapCommand {
    fn profile(&self) -> Option<&ProfileName> {
        self.cloud.profile_name()
    }

    /// The short cloud name, as passed to hooks and used as a config key prefix
    fn cloud_name(&self) -> &str {
        self.cloud.short_name()
    }

    fn custom_mapper_name(&self) -> Option<&str> {
        match &self.cloud {
            Cloud::Custom(name) => Some(name),
            _ => None,
        }
    }

    /// Whether this instance speaks Cumulocity
    /// (the built-in cloud, or a custom-named instance typed c8y)
    fn is_c8y(&self) -> bool {
        self.cloud_name() == "c8y" || self.cloud_type.as_deref() == Some("c8y")
    }

    fn mapper_dir(&self, name: &str) -> Utf8PathBuf {
        self.config_dir.join("mappers").join(name)
    }

    fn mapper_config_path(&self, name: &str) -> Utf8PathBuf {
        MapperToml::path_for(&self.config_dir, name)
    }

    /// The instance-scoped config key of a setting:
    /// `c8y.<setting>`, `c8y.profiles.<p>.<setting>`, or `<mapper>.<setting>`
    fn instance_key(&self, setting: &str) -> String {
        format!("{}.{setting}", self.cloud.config_prefix())
    }

    /// Build a cloud config key, qualified with the cloud profile if one is used
    fn config_key(&self, setting: &str, value: String) -> KeyValue {
        KeyValue {
            key: self.instance_key(setting),
            value,
        }
    }

    /// A non-empty instance-scoped setting of a built-in cloud,
    /// read from the tedge config
    fn read_instance_setting(&self, config: &TEdgeConfig, setting: &str) -> Option<String> {
        let key = self.instance_key(setting).parse::<ReadableKey>().ok()?;
        config
            .read_string(&key)
            .ok()
            .filter(|value| !value.is_empty())
    }

    /// Re-load the configuration from disk.
    ///
    /// The `TEdgeConfig` passed around is an immutable snapshot,
    /// so it must be re-loaded after each step that updates the config.
    async fn load_config(&self) -> anyhow::Result<TEdgeConfig> {
        TEdgeConfig::load(&self.config_dir)
            .await
            .context("Failed to reload the tedge configuration")
    }
}

fn pairs(updates: &[(String, String)]) -> impl Iterator<Item = (&str, &str)> {
    updates
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
}

/// A registration method needs a device identity and none could be found
fn missing_device_id_error(method: &str) -> anyhow::Error {
    anyhow!(
        "A device id is required for {method} registration; \
         provide one with --device-id (or TEDGE_DEVICE_ID)"
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
    use super::*;
    use std::sync::Mutex;
    use tedge_system_services::SystemServiceError;

    #[test]
    fn secret_looking_values_are_masked() {
        assert_eq!(display_value("proxy.password", "hunter2"), "********");
        assert_eq!(display_value("acme.api_token", "abc"), "********");
        assert_eq!(display_value("device.cryptoki.pin", "1234"), "********");
        assert_eq!(display_value("c8y.url", "example.com"), "example.com");
        // an unset secret is shown as such
        assert_eq!(display_value("proxy.password", ""), "");
    }

    /// Records the service operations of a run instead of performing them
    #[derive(Debug, Default)]
    struct StubServiceManager {
        calls: Mutex<Vec<String>>,
    }

    impl StubServiceManager {
        fn record(&self, operation: &str, service: SystemService<'_>) {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{operation} {}", service.name));
        }
    }

    #[async_trait::async_trait]
    impl SystemServiceManager for StubServiceManager {
        fn name(&self) -> &str {
            "stub"
        }
        async fn check_operational(&self) -> Result<(), SystemServiceError> {
            Ok(())
        }
        async fn stop_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
            self.record("stop", service);
            Ok(())
        }
        async fn start_service(
            &self,
            service: SystemService<'_>,
        ) -> Result<(), SystemServiceError> {
            self.record("start", service);
            Ok(())
        }
        async fn restart_service(
            &self,
            service: SystemService<'_>,
        ) -> Result<(), SystemServiceError> {
            self.record("restart", service);
            Ok(())
        }
        async fn enable_service(
            &self,
            service: SystemService<'_>,
        ) -> Result<(), SystemServiceError> {
            self.record("enable", service);
            Ok(())
        }
        async fn disable_service(
            &self,
            service: SystemService<'_>,
        ) -> Result<(), SystemServiceError> {
            self.record("disable", service);
            Ok(())
        }
        async fn is_service_running(
            &self,
            _service: SystemService<'_>,
        ) -> Result<bool, SystemServiceError> {
            Ok(false)
        }
    }

    /// A device config directory with a tedge.toml, and commands against it
    struct Fixture {
        _tmp: tempfile::TempDir,
        config_dir: Utf8PathBuf,
        services: Arc<StubServiceManager>,
    }

    impl Fixture {
        async fn new(tedge_toml: &str) -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let config_dir = Utf8PathBuf::try_from(tmp.path().to_owned()).unwrap();
            tokio::fs::write(config_dir.join("tedge.toml"), tedge_toml)
                .await
                .unwrap();
            Self {
                _tmp: tmp,
                config_dir,
                services: Arc::new(StubServiceManager::default()),
            }
        }

        async fn config(&self) -> TEdgeConfig {
            TEdgeConfig::load(&self.config_dir).await.unwrap()
        }

        fn read(&self, config: &TEdgeConfig, key: &str) -> Option<String> {
            config
                .read_string(&key.parse().unwrap())
                .ok()
                .filter(|value| !value.is_empty())
        }

        fn command(&self, cloud: Cloud) -> BootstrapCommand {
            BootstrapCommand {
                config_dir: self.config_dir.clone(),
                plugin_paths: Vec::new(),
                service_manager: self.services.clone(),
                invocation: Invocation {
                    cloud: cloud.short_name().to_owned(),
                    profile: None,
                    cloud_type: None,
                    url: None,
                    register: None,
                    device_id: None,
                    set: Default::default(),
                    env: Vec::new(),
                    re_register: false,
                    clean: false,
                },
                cloud,
                cloud_type: None,
                url: None,
                register: RegistrationMethod::C8yCa,
                device_id: None,
                one_time_password: OneTimePassword::None,
                settings: Vec::new(),
                method_settings: Vec::new(),
                connect_timeout: None,
                hook_envs: Vec::new(),
                re_register: false,
                clean: false,
                offline: false,
                save_path: None,
                ui: Arc::new(Ui::new(false, Some(self.config_dir.clone().into()), true)),
                dry_run: false,
            }
        }
    }

    fn touch(path: &Utf8Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "content").unwrap();
    }

    #[tokio::test]
    async fn clean_unwinds_only_the_bootstrap_managed_keys_of_a_builtin_cloud() {
        let fx = Fixture::new(
            r#"
[c8y]
url = "example.cumulocity.com"
auth_method = "basic"
credentials_path = "/etc/tedge/mappers/c8y/credentials.toml"
mqtt_service.enabled = true
software_management.api = "advanced"

[c8y.profiles.prod]
url = "prod.example.com"
"#,
        )
        .await;
        let mut command = fx.command(Cloud::c8y(None));
        command.clean = true;
        // the run's own settings are unwound too (they are re-applied);
        // a device-global --set is not the instance's to remove
        command.settings = vec![
            KeyValue {
                key: "c8y.mqtt_service.enabled".into(),
                value: "true".into(),
            },
            KeyValue {
                key: "proxy.address".into(),
                value: "proxy.example.com".into(),
            },
        ];

        let config = command
            .unwind_instance_config(fx.config().await)
            .await
            .unwrap();
        assert_eq!(fx.read(&config, "c8y.url"), None);
        assert_eq!(
            fx.read(&config, "c8y.auth_method").as_deref(),
            Some("certificate")
        );
        assert_eq!(
            fx.read(&config, "c8y.mqtt_service.enabled").as_deref(),
            Some("false")
        );
        // the user's own settings of the section survive
        assert_eq!(
            fx.read(&config, "c8y.software_management.api").as_deref(),
            Some("advanced")
        );
        // other instances are untouched
        assert_eq!(
            fx.read(&config, "c8y.profiles.prod.url").as_deref(),
            Some("prod.example.com")
        );
    }

    #[tokio::test]
    async fn clean_unwinds_only_the_bootstrap_managed_keys_of_a_mapper_toml() {
        let fx = Fixture::new("").await;
        let path = MapperToml::path_for(&fx.config_dir, "acme");
        write_mapper_config(
            &path,
            &[
                ("url".to_owned(), "acme.example.com".to_owned()),
                ("device.id".to_owned(), "acme01".to_owned()),
                ("bridge.topic_prefix".to_owned(), "acme".to_owned()),
                ("bridge.custom_rule".to_owned(), "keep-me".to_owned()),
                ("transport.port".to_owned(), "8883".to_owned()),
            ],
        )
        .await
        .unwrap();
        let mut command = fx.command(Cloud::Custom("acme".into()));
        command.clean = true;
        command.settings = vec![KeyValue {
            key: "acme.transport.port".into(),
            value: "8883".into(),
        }];

        command
            .unwind_instance_config(fx.config().await)
            .await
            .unwrap();
        let mapper_toml = MapperToml::load(&path).await.unwrap();
        assert_eq!(mapper_toml.url(), None);
        assert_eq!(mapper_toml.device_id(), None);
        assert_eq!(mapper_toml.get(&["bridge", "topic_prefix"]), None);
        assert_eq!(mapper_toml.get(&["transport", "port"]), None);
        // package-shipped content survives
        assert_eq!(
            mapper_toml.get_str(&["bridge", "custom_rule"]),
            Some("keep-me")
        );
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
            &[("device.cert_path".to_owned(), shared_cert.to_string())],
        )
        .await
        .unwrap();
        assert!(command.registration_present(&config).await);

        // its credentials file counts, at the configured (relative) path
        write_mapper_config(
            &path,
            &[
                ("device.cert_path".to_owned(), String::new()),
                (
                    "credentials_path".to_owned(),
                    "secrets/acme.toml".to_owned(),
                ),
            ],
        )
        .await
        .unwrap();
        assert!(!command.registration_present(&config).await);
        touch(&fx.config_dir.join("mappers/acme/secrets/acme.toml"));
        assert!(command.registration_present(&config).await);
    }

    #[tokio::test]
    async fn re_register_removes_the_instance_artifacts_and_stops_its_mapper() {
        let fx = Fixture::new("").await;
        let mut command = fx.command(Cloud::Custom("acme".into()));
        command.re_register = true;
        let config = fx.config().await;
        let credentials = fx.config_dir.join("mappers/acme/credentials.toml");
        let instance_cert = fx
            .config_dir
            .join("mappers/acme/device-certs/tedge-certificate.pem");
        let shared_cert: Utf8PathBuf = config.device_cert_path(None::<&Cloud>).unwrap().into();
        touch(&credentials);
        touch(&instance_cert);
        touch(&shared_cert);

        command
            .remove_registration_artifacts(&config)
            .await
            .unwrap();
        assert!(!credentials.exists());
        assert!(!instance_cert.exists());
        // a custom-named instance never removes the shared certificate
        assert!(shared_cert.exists());
        assert_eq!(
            *fx.services.calls.lock().unwrap(),
            vec!["stop tedge-mapper-acme", "disable tedge-mapper-acme"]
        );
    }

    #[tokio::test]
    async fn the_pending_registration_url_is_exposed_only_for_generated_passwords() {
        let fx = Fixture::new("[c8y]\nurl = \"example.cumulocity.com\"\n").await;
        let mut command = fx.command(Cloud::c8y(None));
        command.device_id = Some("demo01".into());
        let config = fx.config().await;

        // a password supplied by the user stays secret
        command.one_time_password = OneTimePassword::Supplied("s3cret".into());
        assert_eq!(command.pending_registration_url(&config).await, None);

        command.one_time_password = OneTimePassword::Generated("generated".into());
        let url = command.pending_registration_url(&config).await.unwrap();
        assert!(url.contains("example.cumulocity.com"), "{url}");
        assert!(url.contains("demo01"), "{url}");
        assert!(url.contains("generated"), "{url}");

        // an existing certificate means no registration is pending,
        // unless the run re-registers
        let cert: Utf8PathBuf = config
            .device_cert_path(Some(&command.cloud))
            .unwrap()
            .into();
        touch(&cert);
        assert_eq!(command.pending_registration_url(&config).await, None);
        command.re_register = true;
        assert!(command.pending_registration_url(&config).await.is_some());

        // only the c8y-ca method registers this way
        command.register = RegistrationMethod::Basic;
        assert_eq!(command.pending_registration_url(&config).await, None);
    }

    #[tokio::test]
    async fn instance_updates_land_where_the_instance_keeps_its_config() {
        let fx = Fixture::new("").await;
        let updates = [
            ("auth_method".to_owned(), "basic".to_owned()),
            ("device.id".to_owned(), "demo01".to_owned()),
        ];

        // a built-in cloud: its config keys, the device id being global
        let command = fx.command(Cloud::c8y(None));
        command.apply_instance_updates(&updates).await.unwrap();
        let config = fx.config().await;
        assert_eq!(
            fx.read(&config, "c8y.auth_method").as_deref(),
            Some("basic")
        );
        assert_eq!(fx.read(&config, "device.id").as_deref(), Some("demo01"));

        // a custom mapper: its mapper.toml
        let command = fx.command(Cloud::Custom("acme".into()));
        command.apply_instance_updates(&updates).await.unwrap();
        let mapper_toml = MapperToml::load(&MapperToml::path_for(&fx.config_dir, "acme"))
            .await
            .unwrap();
        assert_eq!(mapper_toml.get_str(&["auth_method"]), Some("basic"));
        assert_eq!(mapper_toml.device_id(), Some("demo01"));

        // a dry run changes nothing
        let mut command = fx.command(Cloud::Custom("dry".into()));
        command.dry_run = true;
        command.apply_instance_updates(&updates).await.unwrap();
        assert!(!MapperToml::path_for(&fx.config_dir, "dry").exists());
    }
}
