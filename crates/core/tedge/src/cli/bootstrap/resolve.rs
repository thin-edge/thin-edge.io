//! From the effective arguments of one instance to a runnable command:
//! resolving the cloud (and its type), the registration method against
//! the cloud descriptor, the URL, the registration inputs,
//! and capturing the effective invocation for `--save`

use super::c8y;
use super::command::BootstrapCommand;
use super::command::OneTimePassword;
use super::descriptor;
use super::descriptor::env_var;
use super::descriptor::CloudDescriptor;
use super::hooks;
use super::invocation::Invocation;
use super::mapper_toml::MapperToml;
use super::settings::key;
use super::settings::KeyValue;
use super::ui::Ui;
use super::wizard;
use super::wizard::Prompter;
use crate::cli::common::Cloud;
use anyhow::anyhow;
use camino::Utf8PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_system_services::service_manager;

/// The registration method name delegating to the hooks
/// without selecting one of their methods
pub const HOOK_METHOD: &str = "hook";

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

impl RegistrationMethod {
    /// The built-in method of that name, if any
    fn builtin(name: &str) -> Option<Self> {
        match name {
            c8y::method::CA => Some(Self::C8yCa),
            c8y::method::SELF_SIGNED => Some(Self::SelfSigned),
            c8y::method::BASIC => Some(Self::Basic),
            c8y::method::BASIC_PREREGISTERED => Some(Self::BasicPreregistered),
            _ => None,
        }
    }

    /// The method's name, as cloud vocabulary
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::C8yCa => Some(c8y::method::CA),
            Self::SelfSigned => Some(c8y::method::SELF_SIGNED),
            Self::Basic => Some(c8y::method::BASIC),
            Self::BasicPreregistered => Some(c8y::method::BASIC_PREREGISTERED),
            Self::Hook { method } => method.as_deref(),
        }
    }
}

/// The resolved per-instance arguments of one bootstrap run,
/// after flags, wizard answers, or a --from file have been merged
pub struct EffectiveArgs {
    pub cloud_name: String,
    pub profile: Option<ProfileName>,
    pub cloud_type_flag: Option<String>,
    pub url: Option<String>,
    pub register: Option<String>,
    pub device_id: Option<String>,
    pub settings: Vec<KeyValue>,
    pub hook_envs: Vec<(String, String)>,
    /// The environment variables a replayed invocation was captured with
    /// (by name): they must be set when the run actually registers
    pub replay_env: Vec<String>,
    pub re_register: bool,
    pub clean: bool,
    /// The answers were collected by the interactive wizard,
    /// so the run prints its equivalent non-interactive command
    pub from_wizard: bool,
}

/// The run-shape flags, shared by every instance of a run
#[derive(Clone, Copy)]
pub struct RunOptions {
    pub offline: bool,
    pub dry_run: bool,
    /// `None` for a single connection check attempt (--no-wait)
    pub connect_timeout: Option<Duration>,
}

/// Resolve one instance's arguments into a runnable bootstrap command.
///
/// With a prompter, the registration method's missing inputs are
/// asked for interactively (when the run will actually register)
pub async fn resolve_command(
    config: &TEdgeConfig,
    descriptors: &[CloudDescriptor],
    plugin_paths: &[Utf8PathBuf],
    args: EffectiveArgs,
    options: RunOptions,
    prompter: Option<&mut Prompter>,
    ui: &Arc<Ui>,
) -> anyhow::Result<BootstrapCommand> {
    let cloud = Cloud::from_name(&args.cloud_name, args.profile.clone());
    let cloud_type = resolve_cloud_type(config, &cloud, &args.cloud_type_flag, descriptors).await;

    // The cloud's own descriptor wins; a custom-named instance
    // without one uses its cloud type's descriptor
    let own_key = cloud.short_name();
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
    let c8y_semantics = own_key == c8y::CLOUD || cloud_type.as_deref() == Some(c8y::CLOUD);

    let register =
        resolve_register_method(c8y_semantics, &cloud, descriptor, args.register.as_deref())?;

    // A URL the cloud descriptor declares without a prompt applies
    // when none was given explicitly (--url always wins)
    let url = args.url.or_else(|| {
        descriptor
            .and_then(|d| d.url.as_ref())
            .and_then(|spec| spec.pinned_value())
            .map(str::to_owned)
    });

    let mut hook_envs = args.hook_envs;

    // The c8y-ca one-time password is a method input
    // ($DEVICE_ONE_TIME_PASSWORD, matching `tedge cert download c8y`),
    // collected by the wizard or supplied via the environment.
    // When absent it is pre-generated, so the registration URL is known
    // before the register step runs and can be exposed to hooks
    let supplied_password = descriptor::input_value(&hook_envs, c8y::env::ONE_TIME_PASSWORD);
    let one_time_password = match (&register, supplied_password) {
        (RegistrationMethod::C8yCa, Some(password)) => OneTimePassword::Supplied(password),
        (RegistrationMethod::C8yCa, None) if !options.offline => {
            OneTimePassword::Generated(crate::cli::certificate::c8y::generate_one_time_password())
        }
        _ => OneTimePassword::None,
    };

    let chosen_method = args
        .register
        .as_deref()
        .or(default_method_name(descriptor))
        .and_then(|name| descriptor.and_then(|d| d.method(name)));

    // Settings the descriptor pins (prompt = false) apply without asking,
    // e.g. a derived cloud pinning its transport: cloud-relative ones
    // ahead of the chosen method's implied values, device-global ones
    // as plain settings - an explicit --set wins over both
    let mut method_settings: Vec<KeyValue> = Vec::new();
    let mut pinned_global: Vec<KeyValue> = Vec::new();
    for setting in descriptor
        .map(|d| d.settings.as_slice())
        .unwrap_or_default()
    {
        let Some(value) = setting.pinned_value() else {
            continue;
        };
        if setting.global {
            if !args.settings.iter().any(|s| s.key == setting.key) {
                pinned_global.push(KeyValue::new(&setting.key, value));
            }
        } else {
            method_settings.push(KeyValue::new(&setting.key, value));
        }
    }

    // Capture the effective invocation before defaults are folded in:
    // environment variables by name only — wizard-collected inputs,
    // plus the chosen method's inputs provided by the environment.
    // The device id may come from the TEDGE_DEVICE_ID environment
    // override (the variable bootstrap itself exports to hooks).
    // It behaves like --device-id at runtime, but the capture differs:
    // a flag bakes the id into the saved invocation (device-specific),
    // while the env variable is captured by *name* -
    // keeping the saved file fleet-generic,
    // with the id supplied per device at replay time
    let env_device_id = env_var(hooks::env::DEVICE_ID);
    let device_id = args.device_id.clone().or(env_device_id.clone());

    let mut env_names: Vec<String> = hook_envs.iter().map(|(env, _)| env.clone()).collect();
    if args.device_id.is_none() && env_device_id.is_some() {
        env_names.push(hooks::env::DEVICE_ID.to_owned());
    }
    if let Some(method) = chosen_method {
        for input in &method.inputs {
            let provided = env_var(&input.env).is_some();
            // An offline run defers registration, so it collects no
            // inputs - but its saved invocation is the recipe for the
            // online completion run: list the method's required inputs
            // by name, so `--from` checks them upfront before replaying
            let required_later = options.offline && input.is_required() && input.default.is_none();
            if (provided || required_later) && !env_names.contains(&input.env) {
                env_names.push(input.env.clone());
            }
        }
    }
    let invocation = Invocation {
        cloud: args.cloud_name.clone(),
        profile: args.profile.as_ref().map(|profile| profile.to_string()),
        cloud_type: args.cloud_type_flag.clone(),
        url: url.clone(),
        register: register.name().map(str::to_owned),
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
        hook_envs.extend(descriptor::default_input_envs(method, &hook_envs, env_var));
        method_settings.extend(
            method
                .set_config
                .iter()
                .map(|(key, value)| KeyValue::new(key, value)),
        );
    }

    // Pinned device-global settings are descriptor metadata:
    // applied on every run, but not baked into the saved invocation
    let settings = [args.settings, pinned_global].concat();

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
        settings,
        method_settings,
        hook_envs,
        connect_timeout: options.connect_timeout,
        re_register: args.re_register,
        clean: args.clean,
        offline: options.offline,
        invocation,
        ui: ui.clone(),
        dry_run: options.dry_run,
    };

    // Registration inputs are *required to register*: they are validated —
    // and, on an interactive run, prompted for — only when this run
    // will actually register (no artifacts yet, --re-register, or --clean),
    // so idempotent re-runs never demand secrets they will not use.
    // An offline run defers registration, so its inputs are
    // neither prompted for nor validated - except for
    // basic-preregistered, which registers offline (no exchange)
    // and therefore needs its inputs regardless
    let registers_offline = matches!(command.register, RegistrationMethod::BasicPreregistered);
    let registering = (!options.offline || registers_offline)
        && (command.re_register || command.clean || !command.registration_present(config).await);

    // A replayed invocation was captured with environment variables
    // by name only: fail upfront when the environment does not
    // provide what the capture relied on
    if registering {
        let unset: Vec<&str> = args
            .replay_env
            .iter()
            .map(String::as_str)
            .filter(|env| env_var(env).is_none())
            .collect();
        if !unset.is_empty() {
            return Err(anyhow!(
                "The invocation for {} was captured with environment \
                 variables that are not set: {}. \
                 Export them before replaying",
                args.cloud_name,
                unset.join(", ")
            ));
        }
    }

    let mut prompted = false;
    if let (Some(method), true) = (chosen_method, registering) {
        if let Some(prompter) = prompter {
            let collected = wizard::collect_missing_inputs(method, &command.hook_envs, prompter)?;
            prompted = !collected.is_empty();
            for (env, _) in &collected {
                if !command.invocation.env.contains(env) {
                    command.invocation.env.push(env.clone());
                }
            }
            command.hook_envs.extend(collected);
        }
        let missing = descriptor::missing_inputs(method, &command.hook_envs, env_var);
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
            ));
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

/// The descriptor driving a named cloud's questions and validation:
/// the cloud's own, else its cloud type's
/// (a custom-named instance, e.g. `--type c8y`)
pub async fn descriptor_key(
    config: &TEdgeConfig,
    name: &str,
    profile: Option<ProfileName>,
    cloud_type_flag: &Option<String>,
    descriptors: &[CloudDescriptor],
) -> String {
    let cloud = Cloud::from_name(name, profile);
    let own = cloud.short_name().to_owned();
    if descriptors.iter().any(|d| d.cloud == own) {
        return own;
    }
    resolve_cloud_type(config, &cloud, cloud_type_flag, descriptors)
        .await
        .unwrap_or(own)
}

/// Wizard-collected settings are keyed by the descriptor that asked the
/// questions; move them to the instance's own config prefix
/// (a custom-named instance's mapper config, or a profile's keys)
pub fn retarget_settings(settings: &mut [KeyValue], from_prefix: &str, to_prefix: &str) {
    if from_prefix == to_prefix {
        return;
    }
    let old_prefix = format!("{from_prefix}.");
    for setting in settings {
        if let Some(rest) = setting.key.strip_prefix(&old_prefix) {
            setting.key = format!("{to_prefix}.{rest}");
        }
    }
}

/// The cloud URL already present in the device's configuration, if any
///
/// Used to decide whether an interactive run still needs to ask for it:
/// custom mappers keep it in their mapper.toml,
/// built-in clouds in the tedge config
/// (for c8y, a configured `http` endpoint counts too)
pub async fn configured_url(config: &TEdgeConfig, cloud: &Cloud) -> Option<String> {
    match cloud {
        Cloud::Custom(name) => {
            MapperToml::load_or_empty(&MapperToml::path_for(config.root_dir(), name))
                .await
                .url()
                .map(str::to_owned)
        }
        _ => {
            let prefix = cloud.config_prefix();
            [key::URL, key::HTTP].iter().find_map(|setting| {
                super::command::read_config_string(config, &format!("{prefix}.{setting}"))
            })
        }
    }
}

fn default_method_name(descriptor: Option<&CloudDescriptor>) -> Option<&str> {
    descriptor
        .and_then(CloudDescriptor::default_method)
        .map(|method| method.name.as_str())
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
    let persisted = MapperToml::load_or_empty(&MapperToml::path_for(config.root_dir(), name))
        .await
        .cloud_type()
        .map(str::to_owned);
    persisted.or_else(|| {
        descriptors
            .iter()
            .find(|descriptor| descriptor.cloud == *name)
            .and_then(|descriptor| descriptor.cloud_type.clone())
    })
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
            .or(default_method_name(descriptor))
            .unwrap_or(c8y::method::CA);
        if let Some(builtin) = RegistrationMethod::builtin(raw) {
            return Ok(builtin);
        }
        // No anonymous `hook` bucket for c8y: hook-executed methods
        // must be *declared* by a descriptor override, so they carry
        // a name, a description, and validated inputs like any other method
        return match descriptor.and_then(|d| d.method(raw)) {
            Some(_) => Ok(RegistrationMethod::Hook {
                method: Some(raw.to_owned()),
            }),
            None => Err(unknown_method_error(cloud, descriptor, raw)),
        };
    }

    match raw.or(default_method_name(descriptor)) {
        None | Some(HOOK_METHOD) => Ok(RegistrationMethod::Hook { method: None }),
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
        .unwrap_or_else(|| HOOK_METHOD.to_owned());
    anyhow!("Unknown registration method {method:?} for {cloud}; available: {available}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::bootstrap::descriptor::builtin_descriptors;
    use crate::cli::bootstrap::mapper_toml::write_mapper_config;

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
        // even without any descriptor
        assert_eq!(
            resolve_register_method(true, &c8y, None, None).unwrap(),
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

    #[test]
    fn wizard_settings_are_retargeted_to_the_instance_prefix() {
        let mut settings = vec![
            KeyValue::new("c8y.mqtt_service.enabled", "true"),
            KeyValue::new("proxy.address", "proxy:3128"),
        ];
        // a profile: the wizard asked c8y's questions, the profile owns the answers
        retarget_settings(&mut settings, "c8y", "c8y.profiles.prod");
        assert_eq!(settings[0].key, "c8y.profiles.prod.mqtt_service.enabled");
        // global keys are left alone
        assert_eq!(settings[1].key, "proxy.address");

        // a custom-named c8y instance owns its answers in its mapper config
        let mut settings = vec![KeyValue::new("c8y.mqtt_service.enabled", "true")];
        retarget_settings(&mut settings, "c8y", "c8y-second");
        assert_eq!(settings[0].key, "c8y-second.mqtt_service.enabled");
    }

    #[tokio::test]
    async fn cloud_type_comes_from_the_flag_then_mapper_toml_then_descriptor() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
        let config = TEdgeConfig::load_toml_str_with_root_dir(config_dir, "");
        let descriptors: Vec<CloudDescriptor> = vec![toml::from_str(
            r#"
cloud = "acme"
type = "c8y"
"#,
        )
        .unwrap()];
        let acme = Cloud::Custom("acme".into());

        // built-in clouds have no cloud type
        assert_eq!(
            resolve_cloud_type(&config, &Cloud::c8y(None), &Some("az".into()), &descriptors).await,
            None
        );
        // the descriptor's declared type
        assert_eq!(
            resolve_cloud_type(&config, &acme, &None, &descriptors)
                .await
                .as_deref(),
            Some("c8y")
        );
        // the persisted type wins over the descriptor
        write_mapper_config(
            &MapperToml::path_for(config_dir, "acme"),
            &[KeyValue::new("cloud_type", "az")],
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_cloud_type(&config, &acme, &None, &descriptors)
                .await
                .as_deref(),
            Some("az")
        );
        // the flag wins over everything
        assert_eq!(
            resolve_cloud_type(&config, &acme, &Some("aws".into()), &descriptors)
                .await
                .as_deref(),
            Some("aws")
        );
        // the descriptor driving the questions follows the same resolution
        assert_eq!(
            descriptor_key(&config, "acme", None, &None, &builtin_descriptors()).await,
            "az"
        );
        assert_eq!(
            descriptor_key(&config, "c8y.prod", None, &None, &builtin_descriptors()).await,
            "c8y"
        );
    }

    #[tokio::test]
    async fn configured_urls_are_found_for_every_kind_of_instance() {
        let tmp = tempfile::tempdir().unwrap();
        let config_dir = camino::Utf8Path::from_path(tmp.path()).unwrap();
        let config = TEdgeConfig::load_toml_str_with_root_dir(
            config_dir,
            r#"
[c8y]
http = "http.example.com"

[c8y.profiles.prod]
url = "prod.example.com"
"#,
        );
        // a configured http endpoint counts as a known URL
        // (rendered as host:port, which is fine for a presence check)
        assert_eq!(
            configured_url(&config, &Cloud::c8y(None)).await.as_deref(),
            Some("http.example.com:443")
        );
        assert_eq!(
            configured_url(&config, &Cloud::c8y(Some("prod".parse().unwrap())))
                .await
                .as_deref(),
            Some("prod.example.com")
        );
        assert_eq!(configured_url(&config, &Cloud::az(None)).await, None);

        let acme = Cloud::Custom("acme".into());
        assert_eq!(configured_url(&config, &acme).await, None);
        write_mapper_config(
            &MapperToml::path_for(config_dir, "acme"),
            &[KeyValue::new("url", "acme.example.com")],
        )
        .await
        .unwrap();
        assert_eq!(
            configured_url(&config, &acme).await.as_deref(),
            Some("acme.example.com")
        );
    }
}
