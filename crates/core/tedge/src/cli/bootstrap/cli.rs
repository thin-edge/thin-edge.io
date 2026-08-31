use super::command::BootstrapCommand;
use super::command::BootstrapSequence;
use super::describe;
use super::descriptor;
use super::descriptor::CloudDescriptor;
use super::invocation;
use super::invocation::Invocation;
use super::ui::Ui;
use super::wizard;
use crate::cli::common::resolve_cloud;
use crate::cli::common::Cloud;
use crate::command::BuildCommand;
use crate::command::Command;
use anyhow::anyhow;
use anyhow::Context;
use std::io::IsTerminal;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::TEdgeConfig;
use tedge_system_services::service_manager;

/// The environment variable carrying the c8y-ca one-time password,
/// shared with `tedge cert download c8y`;
/// declared as an input of the c8y-ca method in the compiled-in descriptor
const ONE_TIME_PASSWORD_ENV: &str = "DEVICE_ONE_TIME_PASSWORD";

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
    #[clap(long = "set", value_parser = parse_key_value, value_name = "KEY=VALUE")]
    settings: Vec<KeyValue>,

    /// The cloud type of a custom-named mapper instance, e.g. c8y.
    ///
    /// Enables the named cloud's registration methods and wizard options
    /// for an instance with a non-default name
    /// (e.g. a second Cumulocity instance: `tedge bootstrap c8y-second --type c8y`),
    /// and is persisted as the instance's cloud_type.
    /// On re-runs it defaults to the cloud_type already in the instance's mapper.toml
    #[clap(long = "type")]
    cloud_type: Option<String>,

    /// Run the interactive wizard even when stdin is not a terminal
    #[clap(long)]
    interactive: bool,

    /// Maximum time to wait for the cloud connection to come up.
    ///
    /// The first connection can be slow (service start, DNS, TLS),
    /// or depend on an operator action (e.g. registering a certificate
    /// in the cloud's UI), so the connection check is retried until then
    #[clap(long, default_value = "5m")]
    #[arg(value_parser = humantime::parse_duration)]
    timeout: std::time::Duration,

    /// Only try the connection check once instead of retrying until --timeout
    #[clap(long = "no-wait")]
    no_wait: bool,

    /// Only print what would be done, without changing anything
    #[clap(long = "dry-run", alias = "dry")]
    dry_run: bool,

    /// Show the full output: skipped hooks, hook invocations,
    /// and the composed steps' own output
    /// (by default summarized, and dumped when a step fails)
    #[clap(long)]
    verbose: bool,

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
    plugin_dir: Vec<camino::Utf8PathBuf>,

    /// Bootstrap from an invocation file: a JSON array of invocations
    /// (the format --save writes), run in order —
    /// e.g. two Cumulocity instances.
    ///
    /// Environment variables are captured by name only:
    /// the listed variables must be set when replaying
    #[clap(long, value_name = "FILE", conflicts_with_all = ["cloud", "url", "register", "device_id", "profile", "cloud_type", "settings", "interactive"])]
    from: Option<camino::Utf8PathBuf>,

    /// Save the effective invocation(s) as a declarative JSON array file,
    /// replayable with --from; append further instances by editing the array.
    ///
    /// Combined with --dry-run: walk the wizard, save the answers,
    /// apply nothing — then apply here or on another device with --from
    #[clap(long, value_name = "FILE")]
    save: Option<camino::Utf8PathBuf>,

    /// Remove the instance's existing registration artifacts first,
    /// so registration re-runs against the kept configuration:
    /// its credentials file and its certificates
    /// (for the default instance this is the shared device certificate,
    /// removed with a warning as other cloud connections may use it).
    /// Hooks receive --re-register so they can re-register too
    #[clap(long = "re-register")]
    re_register: bool,

    /// Unwind the instance completely before bootstrapping:
    /// everything --re-register removes, plus the instance's own
    /// configuration (the cloud's config section, or the bootstrap-managed
    /// keys in its mapper.toml); device-global settings are kept.
    /// The run then needs its inputs supplied afresh.
    /// Hooks receive --clean so they can remove their own state too
    #[clap(long)]
    clean: bool,

    /// Provision without network access: apply the configuration,
    /// run the hooks (which receive --offline), and stage the services,
    /// deferring everything that needs the cloud.
    ///
    /// Registration is deferred for the built-in methods
    /// (registration inputs are not collected,
    /// and no registration URL is printed -
    /// its one-time password would not survive to the online run);
    /// register hooks still run and may fulfil registration offline
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

/// The resolved registration method
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationMethod {
    /// Request a device certificate from the Cumulocity certificate authority
    C8yCa,
    /// Create a self-signed certificate and upload it using user credentials
    SelfSigned,
    /// Username/password device credentials (basic auth),
    /// requested via the bootstrap user exchange
    Basic,
    /// Store pre-registered device credentials (basic auth),
    /// issued out of band - no cloud exchange, so it also works offline
    BasicPreregistered,
    /// Delegate registration to the bootstrap.d/register.d hooks,
    /// optionally selecting one of the methods a cloud's hooks offer
    Hook { method: Option<String> },
}

/// A raw `key=value` pair for `--set`.
///
/// The key is validated at execution time,
/// against tedge config keys for built-in clouds
/// and against the mapper config for custom cloud mappers.
#[derive(Clone, Debug)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

fn parse_key_value(input: &str) -> Result<KeyValue, String> {
    let (key, value) = input
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VALUE, got {input:?}"))?;
    if key.is_empty() {
        return Err(format!("expected KEY=VALUE, got {input:?}"));
    }
    Ok(KeyValue {
        key: key.to_owned(),
        value: value.to_owned(),
    })
}

#[async_trait::async_trait]
impl BuildCommand for TEdgeBootstrapCli {
    async fn build_command(
        self,
        config: &TEdgeConfig,
    ) -> Result<Box<dyn Command>, crate::ConfigError> {
        let plugin_paths = if self.plugin_dir.is_empty() {
            bootstrap_plugin_paths(config)
        } else {
            self.plugin_dir.clone()
        };
        let descriptors = descriptor::load_descriptors(&plugin_paths).await?;

        // Live documentation: render the resolved descriptors and stop
        if self.describe {
            let key = match &self.cloud {
                Some(name) => {
                    // the same resolution the wizard uses: the cloud's own
                    // descriptor, else its cloud type's
                    let cloud = resolve_cloud(name, self.profile.clone())
                        .unwrap_or_else(|| Cloud::Custom(name.clone()));
                    let own = descriptor_key(&cloud).to_owned();
                    match descriptors.iter().any(|d| d.cloud == own) {
                        true => Some(own),
                        false => Some(
                            resolve_cloud_type(config, &cloud, &self.cloud_type, &descriptors)
                                .await
                                .unwrap_or(own),
                        ),
                    }
                }
                None => None,
            };
            let output = describe::render(&descriptors, key.as_deref()).map_err(|e| anyhow!(e))?;
            return Ok(describe::DescribeCommand { output }.into_boxed());
        }

        // Replay previously captured invocations, in file order
        if let Some(from) = &self.from {
            let content = tokio::fs::read_to_string(from)
                .await
                .with_context(|| format!("Failed to read {from}"))
                .map_err(|e| anyhow!(e))?;
            let invocations = invocation::parse_invocations(&content)
                .with_context(|| format!("Invalid invocation file {from}"))
                .map_err(|e| anyhow!(e))?;
            // Secrets are captured by name only: fail upfront when the
            // environment does not provide what the capture relied on
            let missing: std::collections::BTreeSet<&str> = invocations
                .iter()
                .flat_map(|invocation| invocation.env.iter())
                .map(String::as_str)
                .filter(|env| std::env::var(env).map_or(true, |value| value.is_empty()))
                .collect();
            if !missing.is_empty() {
                let missing = missing.into_iter().collect::<Vec<_>>().join(", ");
                return Err(anyhow!(
                    "The invocations in {from} were captured with environment \
                     variables that are not set: {missing}. \
                     Export them before replaying"
                )
                .into());
            }
            let mut commands = Vec::new();
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
                        .map(|(key, value)| KeyValue { key, value })
                        .collect(),
                    hook_envs: Vec::new(),
                    re_register: invocation.re_register || self.re_register,
                    clean: invocation.clean || self.clean,
                    // a replayed file is already the declarative form
                    from_wizard: false,
                };
                commands.push(
                    self.build_one(config, &descriptors, &plugin_paths, args, false)
                        .await?,
                );
            }
            return Ok(BootstrapSequence {
                commands,
                save_path: self.save.clone(),
            }
            .into_boxed());
        }

        let interactive = self.interactive || std::io::stdin().is_terminal();
        let seed_for = |cloud: Option<String>| wizard::WizardSeed {
            cloud,
            url: self.url.clone(),
            register: self.register.clone(),
            device_id: self.device_id.clone(),
            set_keys: self.settings.iter().map(|s| s.key.clone()).collect(),
        };

        let (cloud_name, mut answers, wizard_key) = match &self.cloud {
            Some(name) => {
                // The cloud is known: prompt for the remaining answers
                // only when required information is genuinely missing
                // (an interactive first-time run with no URL from flag,
                // descriptor, or existing configuration) —
                // configured devices and flag-complete invocations
                // stay fully non-interactive
                let cloud = resolve_cloud(name, self.profile.clone())
                    .unwrap_or_else(|| Cloud::Custom(name.clone()));
                // The cloud's own descriptor drives its questions;
                // a custom-named instance without one answers the questions
                // of its cloud type's descriptor (e.g. --type c8y)
                let own = descriptor_key(&cloud).to_owned();
                let key = if descriptors.iter().any(|d| d.cloud == own) {
                    own
                } else {
                    resolve_cloud_type(config, &cloud, &self.cloud_type, &descriptors)
                        .await
                        .unwrap_or(own)
                };
                // A --clean run is a fresh device again: the configured URL
                // is about to be unwound, so it does not count as known
                let url_known = self.url.is_some()
                    || descriptors
                        .iter()
                        .find(|d| d.cloud == key)
                        .and_then(|d| d.url.as_ref())
                        .and_then(|u| u.fixed_value())
                        .is_some()
                    || (!self.clean && configured_url(config, &cloud).await.is_some());
                if interactive && !url_known {
                    let answers = wizard::run(&descriptors, &seed_for(Some(key.clone())))?;
                    (name.clone(), Some(answers), Some(key))
                } else {
                    (name.clone(), None, None)
                }
            }
            None => {
                if !interactive {
                    return Err(anyhow!(
                        "No cloud specified. \
                         Pass a cloud name (e.g. `tedge bootstrap c8y --url ...`), \
                         or run interactively from a terminal (or with --interactive)"
                    )
                    .into());
                }
                let answers = wizard::run(&descriptors, &seed_for(None))?;
                let key = answers.cloud.clone();
                (answers.cloud.clone(), Some(answers), Some(key))
            }
        };

        let cloud = match resolve_cloud(&cloud_name, self.profile.clone()) {
            Some(cloud) => cloud,
            None => Cloud::Custom(cloud_name.clone()),
        };

        // Wizard-collected settings are prefixed with the descriptor key
        // used for the questions; retarget them to the instance's own
        // config prefix (a custom-named instance's mapper config,
        // or a profile's config keys)
        if let (Some(answers), Some(wizard_key)) = (answers.as_mut(), &wizard_key) {
            let target = settings_prefix(&cloud);
            if *wizard_key != target {
                let old_prefix = format!("{wizard_key}.");
                for setting in &mut answers.settings {
                    if let Some(rest) = setting.key.strip_prefix(&old_prefix) {
                        setting.key = format!("{target}.{rest}");
                    }
                }
            }
        }

        let from_wizard = answers.is_some();
        let (register_raw, url, device_id, settings, hook_envs) = match answers {
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
            register: register_raw,
            device_id,
            settings,
            hook_envs,
            re_register: self.re_register,
            clean: self.clean,
            from_wizard,
        };
        let mut command = self
            .build_one(config, &descriptors, &plugin_paths, args, interactive)
            .await?;
        command.save_path = self.save.clone();
        Ok(command.into_boxed())
    }
}

/// The resolved per-instance arguments of one bootstrap run,
/// after flags, wizard answers, or a --from file have been merged
struct EffectiveArgs {
    cloud_name: String,
    profile: Option<ProfileName>,
    cloud_type_flag: Option<String>,
    url: Option<String>,
    register: Option<String>,
    device_id: Option<String>,
    settings: Vec<KeyValue>,
    hook_envs: Vec<(String, String)>,
    re_register: bool,
    clean: bool,
    /// The answers were collected by the interactive wizard,
    /// so the run prints its equivalent non-interactive command
    from_wizard: bool,
}

impl TEdgeBootstrapCli {
    /// Resolve one instance's arguments into a runnable bootstrap command
    async fn build_one(
        &self,
        config: &TEdgeConfig,
        descriptors: &[CloudDescriptor],
        plugin_paths: &[camino::Utf8PathBuf],
        args: EffectiveArgs,
        prompt_missing: bool,
    ) -> Result<BootstrapCommand, crate::ConfigError> {
        let cloud = match resolve_cloud(&args.cloud_name, args.profile.clone()) {
            Some(cloud) => cloud,
            None => Cloud::Custom(args.cloud_name.clone()),
        };
        let cloud_type =
            resolve_cloud_type(config, &cloud, &args.cloud_type_flag, descriptors).await;

        // The cloud's own descriptor wins; a custom-named instance
        // without one uses its cloud type's descriptor
        let own_key = descriptor_key(&cloud).to_owned();
        let descriptor = descriptors
            .iter()
            .find(|descriptor| descriptor.cloud == own_key)
            .or_else(|| {
                cloud_type.as_deref().and_then(|cloud_type| {
                    descriptors
                        .iter()
                        .find(|descriptor| descriptor.cloud == cloud_type)
                })
            });
        let c8y_semantics = own_key == "c8y" || cloud_type.as_deref() == Some("c8y");

        let register =
            resolve_register_method(c8y_semantics, &cloud, descriptor, args.register.as_deref())?;

        // A URL declared fixed by the cloud descriptor applies
        // when none was given explicitly (--url always wins)
        let url = args.url.or_else(|| {
            descriptor
                .and_then(|d| d.url.as_ref())
                .and_then(|spec| spec.fixed_value())
                .map(str::to_owned)
        });

        // Inputs with declared defaults are filled in when not set otherwise,
        // and config values implied by the method are collected
        let mut hook_envs = args.hook_envs;

        // The c8y-ca one-time password is a method input
        // ($DEVICE_ONE_TIME_PASSWORD, matching `tedge cert download c8y`),
        // collected by the wizard or supplied via the environment.
        // When absent it is pre-generated, so the registration URL is known
        // before the register step runs and can be exposed to hooks
        // (QR codes, operator displays);
        // a supplied password is kept secret (not displayed, not in the URL)
        let supplied_password = hook_envs
            .iter()
            .find(|(env, _)| env == ONE_TIME_PASSWORD_ENV)
            .map(|(_, value)| value.clone())
            .or_else(|| {
                std::env::var(ONE_TIME_PASSWORD_ENV)
                    .ok()
                    .filter(|value| !value.is_empty())
            });
        let (one_time_password, generated_one_time_password) = match (&register, supplied_password)
        {
            (RegistrationMethod::C8yCa, Some(password)) => (Some(password), false),
            // no pre-generation on an offline run: registration is deferred,
            // and a printed registration URL's password would not survive
            // to the online run that actually registers
            (RegistrationMethod::C8yCa, None) if !self.offline => (
                Some(crate::cli::certificate::c8y::generate_one_time_password()),
                true,
            ),
            _ => (None, false),
        };
        let chosen_method = args
            .register
            .as_deref()
            .or(default_method_name(&cloud, descriptor))
            .and_then(|name| descriptor.and_then(|d| d.method(name)));
        let mut method_settings = Vec::new();
        if let Some(descriptor) = descriptor {
            // Config values implied by the cloud itself
            // (e.g. a derived cloud pinning its transport),
            // before the method's own implied values
            method_settings.extend(
                descriptor
                    .set_config
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        // Capture the effective invocation before defaults are folded in:
        // environment variables by name only — wizard-collected inputs,
        // plus the chosen method's inputs provided by the environment
        // The device id may come from the TEDGE_DEVICE_ID environment
        // override (the variable bootstrap itself exports to hooks).
        // It behaves like --device-id at runtime, but the capture differs:
        // a flag bakes the id into the saved invocation (device-specific),
        // while the env variable is captured by *name* -
        // keeping the saved file fleet-generic,
        // with the id supplied per device at replay time
        let env_device_id = std::env::var("TEDGE_DEVICE_ID")
            .ok()
            .filter(|id| !id.is_empty());
        let device_id = args.device_id.clone().or(env_device_id.clone());

        let mut env_names: Vec<String> = hook_envs.iter().map(|(env, _)| env.clone()).collect();
        if args.device_id.is_none() && env_device_id.is_some() {
            env_names.push("TEDGE_DEVICE_ID".to_owned());
        }
        if let Some(method) = chosen_method {
            for input in &method.inputs {
                if !env_names.contains(&input.env)
                    && std::env::var(&input.env).is_ok_and(|value| !value.is_empty())
                {
                    env_names.push(input.env.clone());
                }
            }
            // An offline run defers registration, so it collects no inputs -
            // but its saved invocation is the recipe for the online
            // completion run: list the method's required inputs by name,
            // so `--from` checks them upfront before replaying
            if self.offline {
                for input in &method.inputs {
                    if input.is_required()
                        && input.default.is_none()
                        && !env_names.contains(&input.env)
                    {
                        env_names.push(input.env.clone());
                    }
                }
            }
        }
        let invocation = Invocation {
            cloud: args.cloud_name.clone(),
            profile: args.profile.as_ref().map(|profile| profile.to_string()),
            cloud_type: args.cloud_type_flag.clone(),
            url: url.clone(),
            register: register_method_name(&register).map(str::to_owned),
            device_id: args.device_id.clone(),
            set: args
                .settings
                .iter()
                .map(|setting| (setting.key.clone(), setting.value.clone()))
                .collect(),
            env: env_names,
            re_register: args.re_register,
            clean: args.clean,
        };

        if let Some(method) = chosen_method {
            hook_envs.extend(descriptor::default_input_envs(method, &hook_envs));
            method_settings.extend(
                method
                    .set_config
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }

        let mut command = BootstrapCommand {
            config_dir: config.root_dir().to_owned(),
            plugin_paths: plugin_paths.to_vec(),
            service_manager: service_manager(config.root_dir())?,
            cloud,
            cloud_type,
            url,
            register,
            device_id,
            one_time_password,
            generated_one_time_password,
            settings: args.settings,
            method_settings,
            hook_envs,
            connect_timeout: (!self.no_wait).then_some(self.timeout),
            re_register: args.re_register,
            clean: args.clean,
            offline: self.offline,
            invocation,
            save_path: None,
            ui: Ui::new(
                self.verbose,
                Some(std::path::PathBuf::from(config.logs.path.to_string())),
                self.ascii,
            ),
            dry_run: self.dry_run,
        };

        // Method inputs are *required to register*: they are validated —
        // and, on an interactive run, prompted for — only when this run
        // will actually register (no artifacts yet, --re-register, or --clean),
        // so idempotent re-runs never demand secrets they will not use
        let mut prompted = false;
        if let Some(method) = chosen_method {
            // an offline run defers registration, so its inputs are
            // neither prompted for nor validated - except for
            // basic-preregistered, which registers offline (no exchange)
            // and therefore needs its inputs regardless
            let registers_offline =
                matches!(command.register, RegistrationMethod::BasicPreregistered);
            let registering = (!self.offline || registers_offline)
                && (command.re_register
                    || command.clean
                    || !command.registration_present(config).await);
            if registering {
                if prompt_missing {
                    let collected = wizard::collect_missing_inputs(method, &command.hook_envs)
                        .map_err(|e| anyhow!(e))?;
                    prompted = !collected.is_empty();
                    for (env, _) in &collected {
                        if !command.invocation.env.contains(env) {
                            command.invocation.env.push(env.clone());
                        }
                    }
                    command.hook_envs.extend(collected);
                }
                let missing = descriptor::missing_inputs(method, &command.hook_envs);
                if !missing.is_empty() {
                    let missing = missing
                        .iter()
                        .map(|input| format!("{} (${})", input.name, input.env))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(anyhow!(
                        "The {} registration method requires: {missing}. \
                         Set them as environment variables",
                        method.name
                    )
                    .into());
                }
            }
        }

        // Whatever was answered interactively — the full wizard, or only the
        // registration inputs a re-run had to ask for — compiles to the same
        // CLI contract, printed before the pipeline starts
        if args.from_wizard || prompted {
            wizard::print_equivalent_command(&command.invocation);
        }

        Ok(command)
    }
}

/// The resolved registration method's name, as cloud vocabulary
fn register_method_name(method: &RegistrationMethod) -> Option<&str> {
    match method {
        RegistrationMethod::C8yCa => Some("c8y-ca"),
        RegistrationMethod::SelfSigned => Some("self-signed"),
        RegistrationMethod::Basic => Some("basic"),
        RegistrationMethod::BasicPreregistered => Some("basic-preregistered"),
        RegistrationMethod::Hook { method } => method.as_deref(),
    }
}

/// The layered hook and descriptor roots (`bootstrap.plugin_paths`),
/// earlier entries taking precedence per file name
fn bootstrap_plugin_paths(config: &TEdgeConfig) -> Vec<camino::Utf8PathBuf> {
    config
        .bootstrap
        .plugin_paths
        .0
        .iter()
        .map(camino::Utf8PathBuf::from)
        .collect()
}

/// The descriptor lookup key for a cloud
fn descriptor_key(cloud: &Cloud) -> &str {
    match cloud {
        #[cfg(feature = "c8y")]
        Cloud::C8y(_) => "c8y",
        #[cfg(feature = "azure")]
        Cloud::Azure(_) => "az",
        #[cfg(feature = "aws")]
        Cloud::Aws(_) => "aws",
        Cloud::Custom(name) => name,
    }
}

/// The config key prefix the instance's settings target:
/// a custom-named instance's mapper name,
/// a profile's key prefix (e.g. `c8y.profiles.prod`),
/// or the cloud's own key
fn settings_prefix(cloud: &Cloud) -> String {
    match cloud {
        Cloud::Custom(name) => name.clone(),
        cloud => match cloud.profile_name() {
            Some(profile) => format!("{}.profiles.{profile}", descriptor_key(cloud)),
            None => descriptor_key(cloud).to_owned(),
        },
    }
}

/// The cloud URL already present in the device's configuration, if any
///
/// Used to decide whether an interactive run still needs to ask for it:
/// custom mappers keep it in their mapper.toml,
/// built-in clouds in the tedge config
/// (for c8y, a configured `http` endpoint counts too)
async fn configured_url(config: &TEdgeConfig, cloud: &Cloud) -> Option<String> {
    match cloud {
        Cloud::Custom(name) => {
            let mapper_toml = config
                .root_dir()
                .join("mappers")
                .join(name)
                .join("mapper.toml");
            let content = tokio::fs::read_to_string(mapper_toml).await.ok()?;
            let table: toml::Table = content.parse().ok()?;
            table
                .get("url")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .filter(|url| !url.is_empty())
        }
        _ => {
            let prefix = descriptor_key(cloud);
            for setting in ["url", "http"] {
                let key = match cloud.profile_name() {
                    None => format!("{prefix}.{setting}"),
                    Some(profile) => format!("{prefix}.profiles.{profile}.{setting}"),
                };
                let Ok(key) = key.parse::<ReadableKey>() else {
                    continue;
                };
                if let Ok(url) = config.read_string(&key) {
                    if !url.is_empty() {
                        return Some(url);
                    }
                }
            }
            None
        }
    }
}

fn default_method_name<'a>(
    cloud: &Cloud,
    descriptor: Option<&'a CloudDescriptor>,
) -> Option<&'a str> {
    let from_descriptor = descriptor
        .and_then(|d| d.default_method())
        .map(|method| method.name.as_str());
    match cloud {
        #[cfg(feature = "c8y")]
        Cloud::C8y(_) => from_descriptor.or(Some("c8y-ca")),
        _ => from_descriptor,
    }
}

/// The cloud type of a custom-named instance:
/// the --type flag, the cloud_type persisted in its mapper.toml,
/// or the `type` declared by the cloud's own descriptor
async fn resolve_cloud_type(
    config: &TEdgeConfig,
    cloud: &Cloud,
    flag: &Option<String>,
    descriptors: &[CloudDescriptor],
) -> Option<String> {
    let Cloud::Custom(name) = cloud else {
        return None;
    };
    if let Some(cloud_type) = flag {
        return Some(cloud_type.clone());
    }
    let from_mapper_toml = async {
        let mapper_toml = config
            .root_dir()
            .join("mappers")
            .join(name)
            .join("mapper.toml");
        let content = tokio::fs::read_to_string(mapper_toml).await.ok()?;
        let table: toml::Table = content.parse().ok()?;
        table
            .get("cloud_type")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .filter(|cloud_type| !cloud_type.is_empty())
    };
    if let Some(cloud_type) = from_mapper_toml.await {
        return Some(cloud_type);
    }
    descriptors
        .iter()
        .find(|descriptor| descriptor.cloud == *name)
        .and_then(|descriptor| descriptor.cloud_type.clone())
}

fn resolve_register_method(
    c8y_semantics: bool,
    cloud: &Cloud,
    descriptor: Option<&CloudDescriptor>,
    raw: Option<&str>,
) -> anyhow::Result<RegistrationMethod> {
    if c8y_semantics {
        // The built-in method names always resolve;
        // an overriding descriptor can change the default and
        // *add* hook-executed methods (e.g. a vendor PKI),
        // exactly as for custom clouds
        let raw = raw
            .or(default_method_name(cloud, descriptor))
            .unwrap_or("c8y-ca");
        // No anonymous `hook` bucket for c8y: hook-executed methods
        // must be *declared* by a descriptor override, so they carry
        // a name, a description, and validated inputs like any other method
        return match raw {
            "c8y-ca" => Ok(RegistrationMethod::C8yCa),
            "self-signed" => Ok(RegistrationMethod::SelfSigned),
            "basic" => Ok(RegistrationMethod::Basic),
            "basic-preregistered" => Ok(RegistrationMethod::BasicPreregistered),
            other => match descriptor.and_then(|d| d.method(other)) {
                Some(_) => Ok(RegistrationMethod::Hook {
                    method: Some(other.to_owned()),
                }),
                None => Err(unknown_method_error(cloud, descriptor, other)),
            },
        };
    }

    let raw = raw.or(default_method_name(cloud, descriptor));
    match raw {
        None | Some("hook") => Ok(RegistrationMethod::Hook { method: None }),
        Some(name) => {
            if let Some(descriptor) = descriptor {
                if descriptor.method(name).is_none() {
                    return Err(unknown_method_error(cloud, Some(descriptor), name));
                }
            }
            Ok(RegistrationMethod::Hook {
                method: Some(name.to_owned()),
            })
        }
    }
}

fn unknown_method_error(
    cloud: &Cloud,
    descriptor: Option<&CloudDescriptor>,
    method: &str,
) -> anyhow::Error {
    let available = descriptor
        .map(|descriptor| {
            descriptor
                .register
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|names| !names.is_empty())
        .unwrap_or_else(|| "hook".to_owned());
    anyhow!("Unknown registration method {method:?} for {cloud}; available: {available}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::bootstrap::descriptor::builtin_descriptors;

    fn descriptor_for<'a>(
        descriptors: &'a [CloudDescriptor],
        cloud: &str,
    ) -> Option<&'a CloudDescriptor> {
        descriptors.iter().find(|d| d.cloud == cloud)
    }

    #[test]
    fn c8y_defaults_to_ca_and_rejects_unknown_methods() {
        let descriptors = builtin_descriptors();
        let c8y = Cloud::c8y(None);
        let descriptor = descriptor_for(&descriptors, "c8y");
        assert_eq!(
            resolve_register_method(true, &c8y, descriptor, None).unwrap(),
            RegistrationMethod::C8yCa
        );
        let err = resolve_register_method(true, &c8y, descriptor, Some("token")).unwrap_err();
        assert!(err.to_string().contains("available: c8y-ca"), "{err}");
    }

    #[test]
    fn overriding_c8y_descriptor_can_add_hook_methods_and_change_the_default() {
        let descriptor: CloudDescriptor = toml::from_str(
            r#"
cloud = "c8y"
register = [
    { name = "vendor-pki", default = true },
    { name = "c8y-ca" },
]
"#,
        )
        .unwrap();
        let c8y = Cloud::c8y(None);
        // the vendor's hook method resolves, and is the default
        assert_eq!(
            resolve_register_method(true, &c8y, Some(&descriptor), Some("vendor-pki")).unwrap(),
            RegistrationMethod::Hook {
                method: Some("vendor-pki".into())
            }
        );
        assert_eq!(
            resolve_register_method(true, &c8y, Some(&descriptor), None).unwrap(),
            RegistrationMethod::Hook {
                method: Some("vendor-pki".into())
            }
        );
        // built-in names keep working even if omitted from the override
        assert_eq!(
            resolve_register_method(true, &c8y, Some(&descriptor), Some("basic")).unwrap(),
            RegistrationMethod::Basic
        );
    }

    #[test]
    fn custom_cloud_without_descriptor_accepts_any_method() {
        let cloud = Cloud::Custom("thingsboard".into());
        assert_eq!(
            resolve_register_method(false, &cloud, None, Some("token")).unwrap(),
            RegistrationMethod::Hook {
                method: Some("token".into())
            }
        );
        assert_eq!(
            resolve_register_method(false, &cloud, None, None).unwrap(),
            RegistrationMethod::Hook { method: None }
        );
    }

    #[test]
    fn custom_cloud_with_descriptor_validates_and_defaults() {
        let descriptor: CloudDescriptor = toml::from_str(
            r#"
cloud = "thingsboard"
register = [
    { name = "token", default = true },
    { name = "certificate" },
]
"#,
        )
        .unwrap();
        let cloud = Cloud::Custom("thingsboard".into());
        assert_eq!(
            resolve_register_method(false, &cloud, Some(&descriptor), None).unwrap(),
            RegistrationMethod::Hook {
                method: Some("token".into())
            }
        );
        let err =
            resolve_register_method(false, &cloud, Some(&descriptor), Some("nope")).unwrap_err();
        assert!(err.to_string().contains("token, certificate"), "{err}");
    }
}
