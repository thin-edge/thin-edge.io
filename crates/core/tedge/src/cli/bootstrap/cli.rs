use super::command::BootstrapSequence;
use super::describe;
use super::descriptor;
use super::descriptor::CloudDescriptor;
use super::invocation;
use super::resolve;
use super::resolve::EffectiveArgs;
use super::resolve::RunOptions;
use super::settings::KeyValue;
use super::ui::Ui;
use super::wizard;
use super::wizard::Prompter;
use crate::cli::common::is_builtin_cloud;
use crate::cli::common::Cloud;
use crate::command::BuildCommand;
use crate::command::Command;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::io::IsTerminal;
use std::sync::Arc;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

/// Bootstrap the device and onboard it to a cloud (experimental)
///
/// Configures the cloud endpoints, obtains device credentials
/// using the selected registration method, and connects the device.
/// Run without a cloud argument to be guided interactively.
///
/// Custom steps can be added by dropping executables into
/// /usr/share/tedge/bootstrap.d/{prepare,configure,register,finalize}.d/ (packages)
/// or <config-dir>/bootstrap.d/{prepare,configure,register,finalize}.d/ (site);
/// a site hook overrides a packaged hook with the same filename.
/// The searched directories can be changed with
/// `tedge config set bootstrap.plugin_paths <dir>,<dir>,...`
/// (earlier directories take precedence).
///
/// Built-in clouds (c8y, az, aws) are configured via tedge config keys;
/// custom cloud mappers (e.g. thingsboard) are configured via their
/// <config-dir>/mappers/<name>/mapper.toml and registered via register.d hooks,
/// typically shipped by the mapper's own package.
/// The registration methods a cloud offers are declared by cloud descriptors
/// in /usr/share/tedge/bootstrap.d/clouds.d/<cloud>.toml.
#[derive(clap::Args, Debug)]
#[clap(verbatim_doc_comment)]
pub struct TEdgeBootstrapCli {
    /// The cloud to bootstrap: c8y, az, aws (optionally with a profile, e.g. c8y.prod),
    /// or a custom cloud mapper name (e.g. thingsboard).
    ///
    /// When omitted, an interactive wizard guides through the available options
    cloud: Option<String>,

    /// Cloud URL to connect to.
    /// This should be the HTTP/S address used to talk to the platform.
    ///
    /// For Cumulocity, the MQTT endpoint is discovered automatically;
    /// if it differs from the HTTP endpoint, c8y.http and c8y.mqtt
    /// are configured separately instead of c8y.url.
    #[clap(long)]
    url: Option<String>,

    /// How the device obtains its credentials.
    ///
    /// The available methods depend on the cloud
    /// (declared by its cloud descriptor):
    /// c8y offers c8y-ca (default), self-signed, basic and basic-preregistered;
    /// other clouds offer the methods of their register.d hooks
    #[clap(long)]
    register: Option<String>,

    /// The device identifier to be used as the certificate common name
    #[clap(long = "device-id")]
    device_id: Option<String>,

    /// The cloud profile (when the device connects to several instances of a cloud)
    #[clap(long)]
    profile: Option<ProfileName>,

    /// Set additional configuration keys before registering. Can be repeated.
    ///
    /// For built-in clouds these are tedge config keys,
    /// e.g. --set c8y.software_management.api=advanced;
    /// for custom cloud mappers they are mapper config keys
    /// prefixed with the mapper name, e.g. --set thingsboard.transport.port=8883
    #[clap(long = "set", value_parser = KeyValue::parse, value_name = "KEY=VALUE")]
    settings: Vec<KeyValue>,

    /// The cloud type of a custom-named mapper instance, e.g. c8y.
    ///
    /// Enables the named cloud's registration methods and wizard options
    /// for an instance with a non-default name
    /// (e.g. a second Cumulocity instance: `tedge bootstrap c8y-second --type c8y`),
    /// and is persisted as the instance's cloud_type.
    /// When omitted, the cloud_type already in the instance's mapper.toml
    /// or the `type` declared by the cloud's descriptor applies
    #[clap(long = "type")]
    cloud_type: Option<String>,

    /// Run the interactive wizard even when stdin is not a terminal
    #[clap(long)]
    interactive: bool,

    /// Maximum time to wait for a custom cloud mapper to report
    /// a healthy connection.
    ///
    /// The first connection can be slow (service start, DNS, TLS),
    /// or depend on an operator action (e.g. registering a certificate
    /// in the cloud's UI), so the connection check is retried until then.
    /// Built-in clouds use the connect flow's own retries; registration
    /// waits for an operator for up to 10 minutes
    #[clap(long, default_value = "5m")]
    #[arg(value_parser = humantime::parse_duration)]
    timeout: std::time::Duration,

    /// Only try the connection check once instead of retrying until --timeout
    #[clap(long = "no-wait")]
    no_wait: bool,

    /// Only print what would be done, without changing anything
    #[clap(long = "dry-run")]
    dry_run: bool,

    /// Force the plain ASCII output profile
    /// (used automatically when the locale does not advertise UTF-8,
    /// or when TERM=dumb)
    #[clap(long)]
    ascii: bool,

    /// Directory to search for bootstrap hooks (<phase>.d) and
    /// cloud descriptors (clouds.d). Can be repeated;
    /// earlier directories take precedence per file name.
    ///
    /// Overrides the configured bootstrap.plugin_paths
    /// (and the TEDGE_BOOTSTRAP_PLUGIN_PATHS environment variable)
    #[clap(long = "plugin-dir", value_name = "DIR")]
    plugin_dir: Vec<Utf8PathBuf>,

    /// Bootstrap from an invocation file: a JSON array of invocations
    /// (the format --save writes), run in order —
    /// e.g. two Cumulocity instances.
    ///
    /// Environment variables are captured by name only:
    /// the listed variables must be set when replaying
    #[clap(long, value_name = "FILE", conflicts_with_all = ["cloud", "url", "register", "device_id", "profile", "cloud_type", "settings", "interactive"])]
    from: Option<Utf8PathBuf>,

    /// Save the effective invocation(s) as a declarative JSON array file,
    /// replayable with --from; append further instances by editing the array.
    ///
    /// Combined with --dry-run: walk the wizard, save the answers,
    /// apply nothing — then apply here or on another device with --from
    #[clap(long, value_name = "FILE")]
    save: Option<Utf8PathBuf>,

    /// Remove the instance's existing registration artifacts first,
    /// so registration re-runs against the kept configuration:
    /// its credentials file, its certificate and CSR
    /// (for the default instance this is the shared device certificate
    /// and private key, removed with a warning as other cloud
    /// connections may use them).
    /// Hooks receive --re-register so they can re-register too
    #[clap(long = "re-register")]
    re_register: bool,

    /// Unwind the instance before bootstrapping:
    /// everything --re-register removes, plus the configuration
    /// bootstrap manages for the instance (its endpoints, auth method,
    /// credentials location, per-instance defaults, and the settings
    /// this run applies); other settings of the cloud's section and
    /// device-global settings are kept.
    /// The run then needs its inputs supplied afresh.
    /// Hooks receive --clean so they can remove their own state too
    #[clap(long)]
    clean: bool,

    /// Provision without network access: apply the configuration,
    /// run the hooks (which receive --offline), and stage the services,
    /// deferring everything that needs the cloud.
    ///
    /// Registration is deferred for the built-in methods needing the cloud
    /// (their inputs are not collected, and no registration URL is printed -
    /// its one-time password would not survive to the online run);
    /// basic-preregistered stores its credentials offline,
    /// and register hooks still run and may fulfil registration offline
    /// (e.g. a local PKI).
    /// The staged services connect by themselves when the network
    /// appears; re-running the same command online performs the
    /// remaining steps. Not captured by --save
    #[clap(long)]
    offline: bool,

    /// Describe the resolved cloud descriptors instead of bootstrapping:
    /// each cloud's registration methods with their inputs
    /// (as environment variable names) and its settings.
    ///
    /// Rendered from the same descriptors that drive the wizard and
    /// validation - packaged clouds and clouds.d overrides included -
    /// so it documents exactly what this device would ask for
    #[clap(long, conflicts_with_all = ["from", "save", "interactive"])]
    describe: bool,
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeBootstrapCli {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        // A profile only exists for the built-in clouds; combined with
        // a custom name (or `c8y.<profile>`) it would silently create a
        // custom mapper named after the profile
        if let (Some(name), Some(profile)) = (&self.cloud, &self.profile) {
            if !is_builtin_cloud(name) {
                let cloud = name.split('.').next().unwrap_or(name);
                let hint = match is_builtin_cloud(cloud) {
                    true => format!(
                        "use either `tedge bootstrap {name}` or \
                         `tedge bootstrap {cloud} --profile {profile}`"
                    ),
                    false => "custom mappers have no profiles".to_owned(),
                };
                return Err(anyhow!(
                    "--profile applies to a built-in cloud name (c8y, az, aws): {hint}"
                )
                .into());
            }
        }
        let plugin_paths = if self.plugin_dir.is_empty() {
            bootstrap_plugin_paths(config)
        } else {
            self.plugin_dir.clone()
        };
        let descriptors = descriptor::load_descriptors(&plugin_paths).await?;

        if self.describe {
            return self.build_describe(config, &descriptors).await;
        }

        // One console and one log file for the whole run,
        // however many instances it bootstraps
        let ui = Arc::new(Ui::new(
            Some(std::path::PathBuf::from(config.logs.path.to_string())),
            self.ascii,
        ));

        let command = match &self.from {
            Some(from) => {
                self.build_replay(config, &descriptors, &plugin_paths, from, &ui)
                    .await?
            }
            None => {
                self.build_interactive(config, &descriptors, &plugin_paths, &ui)
                    .await?
            }
        };
        Ok(command)
    }
}

impl TEdgeBootstrapCli {
    fn run_options(&self) -> RunOptions {
        RunOptions {
            offline: self.offline,
            dry_run: self.dry_run,
            connect_timeout: (!self.no_wait).then_some(self.timeout),
        }
    }

    /// Live documentation: render the resolved descriptors and stop
    async fn build_describe(
        &self,
        config: &TEdgeConfig,
        descriptors: &[CloudDescriptor],
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        let key = match &self.cloud {
            Some(name) => Some(self.descriptor_key(config, name, descriptors).await),
            None => None,
        };
        let output = describe::render(descriptors, key.as_deref()).map_err(|e| anyhow!(e))?;
        Ok(describe::DescribeCommand { output }.into_boxed())
    }

    /// Replay previously captured invocations, in file order
    async fn build_replay(
        &self,
        config: &TEdgeConfig,
        descriptors: &[CloudDescriptor],
        plugin_paths: &[Utf8PathBuf],
        from: &Utf8Path,
        ui: &Arc<Ui>,
    ) -> anyhow::Result<Box<dyn Command>> {
        let content = tokio::fs::read_to_string(from)
            .await
            .with_context(|| format!("Failed to read {from}"))?;
        let invocations = invocation::parse_invocations(&content)
            .with_context(|| format!("Invalid invocation file {from}"))?;
        let mut commands = Vec::new();
        let mut errors = Vec::new();
        for invocation in invocations {
            let profile = invocation
                .profile
                .map(|profile| {
                    profile
                        .parse::<ProfileName>()
                        .map_err(|e| anyhow!("Invalid profile in {from}: {e}"))
                })
                .transpose()?;
            let args = EffectiveArgs {
                cloud_name: invocation.cloud,
                profile,
                cloud_type_flag: invocation.cloud_type,
                url: invocation.url,
                register: invocation.register,
                device_id: invocation.device_id,
                settings: invocation
                    .set
                    .into_iter()
                    .map(|(key, value)| KeyValue::new(key, value))
                    .collect(),
                hook_envs: Vec::new(),
                // captured by name only: checked when the run registers
                replay_env: invocation.env,
                re_register: invocation.re_register || self.re_register,
                clean: invocation.clean || self.clean,
                // a replayed file is already the declarative form
                from_wizard: false,
            };
            // every invocation is checked before any runs
            match resolve::resolve_command(
                config,
                descriptors,
                plugin_paths,
                args,
                self.run_options(),
                None,
                ui,
            )
            .await
            {
                Ok(command) => commands.push(command),
                Err(err) => errors.push(format!("{err:#}")),
            }
        }
        if !errors.is_empty() {
            return Err(anyhow!("{}", errors.join("\n")));
        }
        Ok(BootstrapSequence {
            commands,
            save_path: self.save.clone(),
        }
        .into_boxed())
    }

    /// A single run from the flags, completed by the wizard where
    /// required information is genuinely missing on an interactive run
    async fn build_interactive(
        &self,
        config: &TEdgeConfig,
        descriptors: &[CloudDescriptor],
        plugin_paths: &[Utf8PathBuf],
        ui: &Arc<Ui>,
    ) -> anyhow::Result<Box<dyn Command>> {
        let interactive = self.interactive || std::io::stdin().is_terminal();
        let mut prompter = interactive.then(Prompter::stdio);
        // The wizard asks in the descriptor's vocabulary (`c8y.<key>`);
        // --set keys are given for the instance (`c8y.profiles.<p>.<key>`),
        // so they are mapped back for the wizard to skip their questions
        let seed_for = |cloud: Option<String>, instance: Option<&Cloud>| {
            let mut set_keys: Vec<KeyValue> = self.settings.clone();
            if let (Some(cloud), Some(instance)) = (&cloud, instance) {
                resolve::retarget_settings(&mut set_keys, &instance.config_prefix(), cloud);
            }
            wizard::WizardSeed {
                cloud,
                url: self.url.clone(),
                register: self.register.clone(),
                device_id: self.device_id.clone(),
                set_keys: set_keys.into_iter().map(|s| s.key).collect(),
            }
        };

        let (cloud_name, mut answers, wizard_key) = match (&self.cloud, prompter.as_mut()) {
            (Some(name), prompter) => {
                // The cloud is known: prompt for the remaining answers
                // only when required information is genuinely missing
                // (an interactive first-time run with no URL from flag,
                // descriptor, or existing configuration) —
                // configured devices and flag-complete invocations
                // stay fully non-interactive
                let cloud = Cloud::from_name(name, self.profile.clone());
                let key = self.descriptor_key(config, name, descriptors).await;
                // A --clean run is a fresh device again: the configured URL
                // is about to be unwound, so it does not count as known
                let url_known = self.url.is_some()
                    || descriptors
                        .iter()
                        .find(|d| d.cloud == key)
                        .and_then(|d| d.url.as_ref())
                        .and_then(|u| u.pinned_value())
                        .is_some()
                    || (!self.clean && resolve::configured_url(config, &cloud).await.is_some());
                match prompter {
                    Some(prompter) if !url_known => {
                        let seed = seed_for(Some(key.clone()), Some(&cloud));
                        let answers = wizard::run(descriptors, &seed, prompter)?;
                        (name.clone(), Some(answers), Some(key))
                    }
                    _ => (name.clone(), None, None),
                }
            }
            (None, None) => {
                return Err(anyhow!(
                    "No cloud specified. \
                     Pass a cloud name (e.g. `tedge bootstrap c8y --url ...`), \
                     or run interactively from a terminal (or with --interactive)"
                ));
            }
            (None, Some(prompter)) => {
                let answers = wizard::run(descriptors, &seed_for(None, None), prompter)?;
                let key = answers.cloud.clone();
                (answers.cloud.clone(), Some(answers), Some(key))
            }
        };

        let cloud = Cloud::from_name(&cloud_name, self.profile.clone());

        // Wizard-collected settings are prefixed with the descriptor key
        // used for the questions; retarget them to the instance's own
        // config prefix (a custom-named instance's mapper config,
        // or a profile's config keys)
        if let (Some(answers), Some(wizard_key)) = (answers.as_mut(), &wizard_key) {
            resolve::retarget_settings(&mut answers.settings, wizard_key, &cloud.config_prefix());
            // an explicit --set always wins over an answer
            answers
                .settings
                .retain(|answer| !self.settings.iter().any(|set| set.key == answer.key));
        }

        let from_wizard = answers.is_some();
        let (register, url, device_id, settings, hook_envs) = match answers {
            Some(answers) => (
                answers.register,
                answers.url,
                answers.device_id,
                [self.settings.clone(), answers.settings].concat(),
                answers.hook_envs,
            ),
            None => (
                self.register.clone(),
                self.url.clone(),
                self.device_id.clone(),
                self.settings.clone(),
                Vec::new(),
            ),
        };

        let args = EffectiveArgs {
            cloud_name,
            profile: self.profile.clone(),
            cloud_type_flag: self.cloud_type.clone(),
            url,
            register,
            device_id,
            settings,
            hook_envs,
            replay_env: Vec::new(),
            re_register: self.re_register,
            clean: self.clean,
            from_wizard,
        };
        let command = resolve::resolve_command(
            config,
            descriptors,
            plugin_paths,
            args,
            self.run_options(),
            prompter.as_mut(),
            ui,
        )
        .await?;
        Ok(BootstrapSequence {
            commands: vec![command],
            save_path: self.save.clone(),
        }
        .into_boxed())
    }

    /// The descriptor driving a named cloud's questions and validation
    async fn descriptor_key(
        &self,
        config: &TEdgeConfig,
        name: &str,
        descriptors: &[CloudDescriptor],
    ) -> String {
        resolve::descriptor_key(
            config,
            name,
            self.profile.clone(),
            &self.cloud_type,
            descriptors,
        )
        .await
    }
}

/// The layered hook and descriptor roots (`bootstrap.plugin_paths`),
/// earlier entries taking precedence per file name
fn bootstrap_plugin_paths(config: &TEdgeConfig) -> Vec<Utf8PathBuf> {
    config
        .bootstrap
        .plugin_paths
        .0
        .iter()
        .map(Utf8PathBuf::from)
        .collect()
}
