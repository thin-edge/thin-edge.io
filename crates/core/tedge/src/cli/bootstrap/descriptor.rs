//! Cloud bootstrap descriptors
//!
//! A descriptor declares, per cloud, which registration methods exist,
//! what inputs they need, and which settings the cloud requires.
//! Mapper packages ship descriptors for their clouds into
//! `<plugin-path>/clouds.d/<cloud>.toml` for each configured
//! `bootstrap.plugin_paths` entry
//! (by default `/usr/share/tedge/bootstrap.d/clouds.d/`,
//! overridable per site in `<config-dir>/bootstrap.d/clouds.d/`);
//! the built-in clouds have descriptors compiled in.
//!
//! Descriptors are metadata only — registration is still *executed*
//! by the built-in methods or the register.d hooks,
//! and the outcome is verified independently.
//! They power the interactive wizard, upfront validation,
//! and error messages that list the available methods.

use anyhow::Context;
use camino::Utf8PathBuf;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CloudDescriptor {
    /// The cloud name, as passed to `tedge bootstrap <cloud>`
    pub cloud: String,
    /// The cloud this descriptor derives from (e.g. `type = "c8y"`):
    /// the instance is bootstrapped as a custom-named instance of that cloud,
    /// with the base cloud's registration semantics,
    /// and inherits the base descriptor's registration methods and URL spec
    /// unless it declares its own
    #[serde(default, rename = "type")]
    pub cloud_type: Option<String>,
    #[serde(default)]
    pub description: String,
    /// The registration methods this cloud supports
    #[serde(default)]
    pub register: Vec<RegisterMethod>,
    /// How the cloud URL is obtained (prompt text, default, or a fixed value)
    #[serde(default)]
    pub url: Option<UrlSpec>,
    /// Settings the cloud needs (beyond the URL)
    #[serde(default)]
    pub settings: Vec<Setting>,
    /// Config values implied by choosing this cloud,
    /// applied during the configure step
    /// (keys are relative to the cloud, e.g. `mqtt_service.enabled`);
    /// method-level `set` values and explicit `--set` values override them
    #[serde(default, rename = "set")]
    pub set_config: BTreeMap<String, String>,
    /// Hidden from the wizard's cloud pick-list and the no-argument
    /// `--describe` listing, so an integrator can curate the offered clouds.
    /// Set by a `clouds.d/<cloud>.ignore` marker file, never from the
    /// descriptor itself; an explicit `tedge bootstrap <cloud>` still works
    #[serde(skip)]
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UrlSpec {
    /// Prompt text shown by the wizard, e.g. "ThingsBoard server (host or host:port)"
    #[serde(default)]
    pub description: String,
    /// Pre-filled value offered by the wizard (accepted with an empty answer)
    #[serde(default)]
    pub default: Option<String>,
    /// When set, the wizard offers these values as a pick-list
    #[serde(default)]
    pub choices: Vec<String>,
    /// The URL is static: the wizard does not prompt for it,
    /// and non-interactive runs use the default without requiring --url.
    /// An explicit --url still overrides it (descriptors are metadata, not policy).
    /// Ignored when no default is declared
    #[serde(default)]
    pub fixed: bool,
}

impl UrlSpec {
    /// The value to use without asking, when the URL is declared fixed
    pub fn fixed_value(&self) -> Option<&str> {
        self.fixed.then_some(self.default.as_deref()).flatten()
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegisterMethod {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Whether this is the default method when `--register` is not given
    #[serde(default)]
    pub default: bool,
    /// Inputs the method needs, passed via environment variables
    #[serde(default)]
    pub inputs: Vec<MethodInput>,
    /// Config values implied by choosing this method,
    /// applied during the configure step
    /// (keys are relative to the cloud, e.g. `auth_method`);
    /// explicit `--set` values override them
    #[serde(default, rename = "set")]
    pub set_config: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MethodInput {
    /// Human readable name, e.g. "provision key"
    pub name: String,
    /// The environment variable carrying the value
    pub env: String,
    /// One-line explanation shown by the wizard and `--describe`
    #[serde(default)]
    pub description: String,
    /// Secret values are prompted without echo and never shown
    #[serde(default)]
    pub secret: bool,
    /// Optional inputs don't fail validation when unset
    #[serde(default)]
    pub required: Option<bool>,
    /// Value used when the environment variable is not set
    /// (applied in both interactive and non-interactive runs)
    #[serde(default)]
    pub default: Option<String>,
    /// When set, the wizard offers these values as a pick-list
    #[serde(default)]
    pub choices: Vec<String>,
}

impl MethodInput {
    pub fn is_required(&self) -> bool {
        self.required.unwrap_or(true)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Setting {
    /// The config key, relative to the cloud (e.g. `transport.port`)
    pub key: String,
    /// The key is a device-global tedge config key (e.g. `proxy.address`),
    /// not scoped to the cloud: the wizard's answer is applied without the
    /// cloud prefix, and lands in the tedge config even for custom mappers
    #[serde(default)]
    pub global: bool,
    /// The human-readable question shown by the wizard
    /// (e.g. "Select the Cumulocity MQTT connection type");
    /// when absent, the wizard shows the key and description instead
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    /// Pre-filled value offered by the wizard (accepted with an empty answer).
    ///
    /// With choices, this selects the pre-selected choice by its *value*.
    /// Wizard-only: shipped configuration defaults belong in the mapper's
    /// own `mapper.toml`, not in the descriptor
    #[serde(default)]
    pub default: Option<String>,
    /// When set, the wizard offers these values as a pick-list.
    ///
    /// A choice is either a bare value (`choices = ["1883", "8883"]`)
    /// or a labeled table presenting the value in product vocabulary
    /// and optionally implying further config values (see [SettingChoice])
    #[serde(default)]
    pub choices: Vec<SettingChoice>,
}

/// One selectable answer of a setting's pick-list
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum SettingChoice {
    /// A bare value, displayed as-is
    Value(String),
    /// A labeled choice: the wizard shows the label and description,
    /// while the `value` is what lands in the setting's key —
    /// so a question can speak product vocabulary
    /// ("MQTT Service") rather than config values ("true")
    Labeled {
        /// The value stored in the setting's key when chosen
        value: String,
        /// The display label (defaults to the value)
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        description: String,
        /// Further config values implied by this choice
        /// (keys relative to the cloud, like the setting's own key);
        /// explicit `--set` values override them
        #[serde(default)]
        set: BTreeMap<String, String>,
    },
}

impl SettingChoice {
    /// The value stored in the setting's key
    pub fn value(&self) -> &str {
        match self {
            Self::Value(value) => value,
            Self::Labeled { value, .. } => value,
        }
    }

    /// The label shown in the pick-list
    pub fn label(&self) -> &str {
        match self {
            Self::Value(value) => value,
            Self::Labeled { label, value, .. } => label.as_deref().unwrap_or(value),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Value(_) => "",
            Self::Labeled { description, .. } => description,
        }
    }

    /// Further config values implied by this choice
    pub fn implied_config(&self) -> Option<&BTreeMap<String, String>> {
        match self {
            Self::Value(_) => None,
            Self::Labeled { set, .. } => Some(set),
        }
    }
}

impl CloudDescriptor {
    pub fn method(&self, name: &str) -> Option<&RegisterMethod> {
        self.register.iter().find(|method| method.name == name)
    }

    pub fn default_method(&self) -> Option<&RegisterMethod> {
        self.register
            .iter()
            .find(|method| method.default)
            .or_else(|| self.register.first())
    }
}

/// The descriptors of the built-in clouds
pub fn builtin_descriptors() -> Vec<CloudDescriptor> {
    let toml = r#"
[[clouds]]
cloud = "c8y"
description = "Cumulocity"

[clouds.url]
description = "Cumulocity URL (the HTTP/S address used to talk to the platform)"

[[clouds.register]]
name = "c8y-ca"
default = true
description = "Request a device certificate from the Cumulocity certificate authority"

[[clouds.register.inputs]]
name = "one-time password"
env = "DEVICE_ONE_TIME_PASSWORD"
description = "Only needed when the device was pre-registered; leave empty to generate one"
secret = true
required = false

[[clouds.register]]
name = "self-signed"
description = "Create a self-signed certificate and upload it using user credentials"

[[clouds.register.inputs]]
name = "Cumulocity username"
env = "C8Y_USER"
required = false

[[clouds.register.inputs]]
name = "Cumulocity password"
env = "C8Y_PASSWORD"
secret = true
required = false

[[clouds.register]]
name = "basic"
description = "Request username/password device credentials via the bootstrap user (device not registered yet)"

[[clouds.register.inputs]]
name = "bootstrap user"
env = "C8Y_BOOTSTRAP_USER"
description = "The tenant's device bootstrap user"
default = "management/devicebootstrap"

[[clouds.register.inputs]]
name = "bootstrap password"
env = "C8Y_BOOTSTRAP_PASSWORD"
description = "The tenant's device bootstrap password, issued by the platform operator"
secret = true

[[clouds.register]]
name = "basic-preregistered"
description = "The device is already pre-registered: store its issued username/password"

[[clouds.register.inputs]]
name = "device username"
env = "C8Y_DEVICE_USER"
description = "The device user, as issued (t<tenant-id>/device_<device-id>)"

[[clouds.register.inputs]]
name = "device password"
env = "C8Y_DEVICE_PASSWORD"
description = "The pre-registered device password"
secret = true

[[clouds.settings]]
key = "mqtt_service.enabled"
name = "Select the Cumulocity MQTT connection type"
default = "false"

[[clouds.settings.choices]]
value = "false"
label = "Core MQTT"
description = "The standard device endpoint (port 8883)"

[[clouds.settings.choices]]
value = "true"
label = "MQTT Service"
description = "Next-gen endpoint with free-form topics (port 9883); requires the mqtt-service.smartrest tenant feature (Public Preview)"

[[clouds]]
cloud = "az"
description = "Azure IoT Hub"

[clouds.url]
description = "Azure IoT Hub hostname (e.g. myhub.azure-devices.net)"

[[clouds.register]]
name = "hook"
default = true
description = "Delegate registration to the bootstrap.d/register.d hooks (e.g. DPS); without hooks, register the certificate thumbprint manually in the IoT Hub portal"

[[clouds]]
cloud = "aws"
description = "AWS IoT Core"

[clouds.url]
description = "AWS IoT Core ATS endpoint (e.g. xxxx-ats.iot.<region>.amazonaws.com)"

[[clouds.register]]
name = "hook"
default = true
description = "Delegate registration to the bootstrap.d/register.d hooks (e.g. fleet provisioning); without hooks, register the thing and certificate manually in AWS IoT Core"
"#;
    #[derive(serde::Deserialize)]
    struct Clouds {
        clouds: Vec<CloudDescriptor>,
    }
    let clouds: Clouds = toml::from_str(toml).expect("builtin cloud descriptors must parse");
    clouds.clouds
}

/// Load all descriptors from each configured `bootstrap.plugin_paths`
/// entry's `clouds.d/` directory, in order
/// (by default `<config-dir>/bootstrap.d/clouds.d/`, then
/// `/usr/share/tedge/bootstrap.d/clouds.d/`).
/// Earlier layers take precedence per cloud name -
/// the convention of `log.plugin_paths` and `configuration.plugin_paths` -
/// with the compiled-in built-ins as the lowest layer.
///
/// A marker file `clouds.d/<cloud>.ignore` (or `<cloud>.toml.ignore`)
/// hides that cloud - built-in or not - from the wizard and the
/// no-argument `--describe` listing, so an integrator can curate
/// what their customers are offered.
/// Visibility follows the same layering:
/// the first layer providing either a descriptor (visible)
/// or a marker (hidden) for a cloud decides,
/// a marker beating a sibling descriptor within its own layer -
/// so a site descriptor re-offers a cloud a package's marker hides.
pub async fn load_descriptors(
    plugin_paths: &[Utf8PathBuf],
) -> anyhow::Result<Vec<CloudDescriptor>> {
    let mut by_cloud: BTreeMap<String, CloudDescriptor> = BTreeMap::new();
    let mut hidden: BTreeMap<String, bool> = BTreeMap::new();
    for dir in plugin_paths.iter().map(|root| root.join("clouds.d")) {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        let mut layer_hidden: BTreeMap<String, bool> = BTreeMap::new();
        let mut markers = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if let Some(marked) = name.strip_suffix(".ignore") {
                let cloud = marked.strip_suffix(".toml").unwrap_or(marked);
                markers.push(cloud.to_owned());
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let content = tokio::fs::read_to_string(&path).await?;
            let descriptor: CloudDescriptor = toml::from_str(&content)
                .with_context(|| format!("Invalid cloud descriptor {}", path.display()))?;
            layer_hidden
                .entry(descriptor.cloud.clone())
                .or_insert(false);
            by_cloud
                .entry(descriptor.cloud.clone())
                .or_insert(descriptor);
        }
        for cloud in markers {
            layer_hidden.insert(cloud, true);
        }
        for (cloud, is_hidden) in layer_hidden {
            hidden.entry(cloud).or_insert(is_hidden);
        }
    }
    for descriptor in builtin_descriptors() {
        by_cloud
            .entry(descriptor.cloud.clone())
            .or_insert(descriptor);
    }
    for (cloud, descriptor) in &mut by_cloud {
        descriptor.hidden = hidden.get(cloud).copied().unwrap_or(false);
    }

    restore_builtin_method_inputs(&mut by_cloud);
    resolve_derived_clouds(&mut by_cloud);
    Ok(by_cloud.into_values().collect())
}

/// The built-in method implementations carry their inputs with them.
///
/// An override restating `c8y-ca`, `self-signed` or `basic` without
/// declaring inputs keeps the compiled-in inputs:
/// the implementation requires those values regardless of how the
/// descriptor presents the method, so dropping them would silently
/// remove the wizard prompts and the upfront validation
/// while the register step still fails without the values.
/// An override declaring its own inputs for a built-in method wins.
///
/// Runs before derived-cloud resolution,
/// so derived clouds inheriting the c8y methods inherit the inputs too.
fn restore_builtin_method_inputs(by_cloud: &mut BTreeMap<String, CloudDescriptor>) {
    let Some(builtin_c8y) = builtin_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.cloud == "c8y")
    else {
        return;
    };
    let Some(c8y) = by_cloud.get_mut("c8y") else {
        return;
    };
    for method in &mut c8y.register {
        if method.inputs.is_empty() {
            if let Some(builtin) = builtin_c8y.method(&method.name) {
                method.inputs = builtin.inputs.clone();
            }
        }
    }
}

/// A derived cloud (`type = "c8y"`) inherits its base cloud's
/// registration methods and URL spec unless it declares its own;
/// its settings and implied config are always its own
/// (the base cloud's questions rarely apply to the derived cloud).
/// Single-level: a base that is itself derived is not resolved further
fn resolve_derived_clouds(by_cloud: &mut BTreeMap<String, CloudDescriptor>) {
    let derived: Vec<String> = by_cloud
        .values()
        .filter(|descriptor| descriptor.cloud_type.is_some())
        .map(|descriptor| descriptor.cloud.clone())
        .collect();
    for name in derived {
        let base = by_cloud
            .get(&name)
            .and_then(|descriptor| descriptor.cloud_type.clone())
            .and_then(|cloud_type| by_cloud.get(&cloud_type).cloned());
        if let Some(base) = base {
            let descriptor = by_cloud.get_mut(&name).expect("derived cloud exists");
            if descriptor.register.is_empty() {
                descriptor.register = base.register;
            }
            if descriptor.url.is_none() {
                descriptor.url = base.url;
            }
        }
    }
}

/// The inputs of a method whose environment variables are not set
/// and which have no default to fall back to
pub fn missing_inputs<'a>(
    method: &'a RegisterMethod,
    extra_envs: &[(String, String)],
) -> Vec<&'a MethodInput> {
    method
        .inputs
        .iter()
        .filter(|input| input.is_required())
        .filter(|input| input.default.is_none())
        .filter(|input| std::env::var(&input.env).map_or(true, |value| value.is_empty()))
        .filter(|input| !extra_envs.iter().any(|(env, _)| env == &input.env))
        .collect()
}

/// The default values of a method's inputs whose environment variables
/// are not otherwise set, to be applied as hook environment variables
pub fn default_input_envs(
    method: &RegisterMethod,
    extra_envs: &[(String, String)],
) -> Vec<(String, String)> {
    method
        .inputs
        .iter()
        .filter_map(|input| {
            let default = input.default.as_ref()?;
            let env_set = std::env::var(&input.env).is_ok_and(|value| !value.is_empty());
            let collected = extra_envs.iter().any(|(env, _)| env == &input.env);
            (!env_set && !collected).then(|| (input.env.clone(), default.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn overrides_restating_builtin_methods_keep_the_compiled_in_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(tmp.path()).unwrap().to_owned();
        std::fs::create_dir_all(root.join("clouds.d")).unwrap();
        // a site override restating basic without inputs, and c8y-ca
        // with its own input declaration
        std::fs::write(
            root.join("clouds.d/c8y.toml"),
            r#"cloud = "c8y"
description = "Cumulocity (site)"

[[register]]
name = "c8y-ca"
default = true

[[register.inputs]]
name = "site token"
env = "SITE_TOKEN"

[[register]]
name = "basic"

[[register]]
name = "vendor-pki"
"#,
        )
        .unwrap();

        let descriptors = load_descriptors(&[root]).await.unwrap();
        let c8y = descriptors.iter().find(|d| d.cloud == "c8y").unwrap();
        // restated without inputs: the compiled-in inputs are kept
        let basic = c8y.method("basic").unwrap();
        assert!(basic.inputs.iter().any(|i| i.env == "C8Y_BOOTSTRAP_USER"));
        assert!(basic
            .inputs
            .iter()
            .any(|i| i.env == "C8Y_BOOTSTRAP_PASSWORD"));
        // an override's own input declaration wins
        let ca = c8y.method("c8y-ca").unwrap();
        assert_eq!(ca.inputs.len(), 1);
        assert_eq!(ca.inputs[0].env, "SITE_TOKEN");
        // non-built-in methods are untouched
        assert!(c8y.method("vendor-pki").unwrap().inputs.is_empty());
    }

    #[tokio::test]
    async fn ignore_markers_hide_clouds_with_first_layer_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let base = camino::Utf8Path::from_path(tmp.path()).unwrap();
        let site = base.join("site");
        let packaged = base.join("packaged");
        std::fs::create_dir_all(site.join("clouds.d")).unwrap();
        std::fs::create_dir_all(packaged.join("clouds.d")).unwrap();

        // a packaged marker hides the built-in az cloud
        std::fs::write(packaged.join("clouds.d/az.ignore"), "").unwrap();
        // the site hides aws (the .toml.ignore spelling works too)
        std::fs::write(site.join("clouds.d/aws.toml.ignore"), "").unwrap();
        // a site descriptor re-offers a cloud a packaged marker hides
        std::fs::write(packaged.join("clouds.d/acme.ignore"), "").unwrap();
        std::fs::write(
            site.join("clouds.d/acme.toml"),
            "cloud = \"acme\"\ndescription = \"Acme\"",
        )
        .unwrap();
        // a marker beats a sibling descriptor in the same layer
        std::fs::write(packaged.join("clouds.d/beta.ignore"), "").unwrap();
        std::fs::write(
            packaged.join("clouds.d/beta.toml"),
            "cloud = \"beta\"\ndescription = \"Beta\"",
        )
        .unwrap();

        let descriptors = load_descriptors(&[site, packaged]).await.unwrap();
        let hidden = |cloud: &str| {
            descriptors
                .iter()
                .find(|d| d.cloud == cloud)
                .unwrap()
                .hidden
        };
        assert!(hidden("az"));
        assert!(hidden("aws"));
        assert!(hidden("beta"));
        assert!(!hidden("acme"));
        // hiding curates the pick-list without removing the cloud
        assert!(!hidden("c8y"));
        assert!(descriptors.iter().any(|d| d.cloud == "az"));
    }

    #[test]
    fn builtin_descriptors_parse_and_have_defaults() {
        let descriptors = builtin_descriptors();
        let c8y = descriptors.iter().find(|d| d.cloud == "c8y").unwrap();
        assert_eq!(c8y.default_method().unwrap().name, "c8y-ca");
        assert!(c8y.method("hook").is_none());
        let az = descriptors.iter().find(|d| d.cloud == "az").unwrap();
        assert_eq!(az.default_method().unwrap().name, "hook");
    }

    #[test]
    fn inputs_with_defaults_are_not_missing_and_are_applied() {
        let method: RegisterMethod = toml::from_str(
            r#"
name = "provision"

[[inputs]]
name = "bootstrap user"
env = "TEST_BOOTSTRAP_USER_UNSET"
default = "management/devicebootstrap"

[[inputs]]
name = "token"
env = "TEST_TOKEN_UNSET"
"#,
        )
        .unwrap();
        let missing = missing_inputs(&method, &[]);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].env, "TEST_TOKEN_UNSET");
        let defaults = default_input_envs(&method, &[]);
        assert_eq!(
            defaults,
            vec![(
                "TEST_BOOTSTRAP_USER_UNSET".to_owned(),
                "management/devicebootstrap".to_owned()
            )]
        );
        // an explicitly collected value suppresses the default
        let collected = vec![("TEST_BOOTSTRAP_USER_UNSET".to_owned(), "other".to_owned())];
        assert!(default_input_envs(&method, &collected).is_empty());
    }

    #[test]
    fn fixed_url_requires_a_default() {
        let fixed: UrlSpec = toml::from_str(
            r#"default = "iot.example.com"
fixed = true"#,
        )
        .unwrap();
        assert_eq!(fixed.fixed_value(), Some("iot.example.com"));
        let prompted: UrlSpec = toml::from_str(r#"default = "iot.example.com""#).unwrap();
        assert_eq!(prompted.fixed_value(), None);
        let fixed_without_default: UrlSpec = toml::from_str("fixed = true").unwrap();
        assert_eq!(fixed_without_default.fixed_value(), None);
    }

    #[test]
    fn setting_choices_parse_as_bare_values_or_labeled_tables() {
        let plain: Setting = toml::from_str(
            r#"
key = "transport.port"
choices = ["1883", "8883"]
"#,
        )
        .unwrap();
        assert_eq!(plain.choices[0].value(), "1883");
        assert_eq!(plain.choices[0].label(), "1883");
        assert!(plain.choices[0].implied_config().is_none());

        let labeled: Setting = toml::from_str(
            r#"
key = "mqtt_service.enabled"
name = "Select the Cumulocity MQTT connection type"
default = "false"

[[choices]]
value = "false"
label = "Core MQTT"
description = "The standard device endpoint (port 8883)"

[[choices]]
value = "true"
label = "MQTT Service"
set = { "mqtt_service.topics" = "demo/topic" }
"#,
        )
        .unwrap();
        assert_eq!(
            labeled.name.as_deref().unwrap(),
            "Select the Cumulocity MQTT connection type"
        );
        assert_eq!(labeled.choices[0].label(), "Core MQTT");
        assert_eq!(labeled.choices[0].value(), "false");
        assert_eq!(labeled.choices[1].label(), "MQTT Service");
        assert_eq!(
            labeled.choices[1]
                .implied_config()
                .unwrap()
                .get("mqtt_service.topics")
                .unwrap(),
            "demo/topic"
        );
        // the builtin c8y descriptor uses the labeled form
        let descriptors = builtin_descriptors();
        let c8y = descriptors.iter().find(|d| d.cloud == "c8y").unwrap();
        assert_eq!(c8y.settings[0].choices[1].label(), "MQTT Service");
        assert_eq!(c8y.settings[0].choices[1].value(), "true");
    }

    #[test]
    fn derived_cloud_inherits_methods_and_url_unless_declared() {
        let mut by_cloud: BTreeMap<String, CloudDescriptor> = builtin_descriptors()
            .into_iter()
            .map(|descriptor| (descriptor.cloud.clone(), descriptor))
            .collect();
        let derived: CloudDescriptor = toml::from_str(
            r#"
cloud = "c8y-service"
type = "c8y"
description = "Cumulocity via the MQTT service"

[set]
"mqtt_service.enabled" = "true"
"#,
        )
        .unwrap();
        assert_eq!(derived.cloud_type.as_deref(), Some("c8y"));
        by_cloud.insert(derived.cloud.clone(), derived);
        resolve_derived_clouds(&mut by_cloud);

        let resolved = &by_cloud["c8y-service"];
        // methods and URL spec inherited from c8y
        assert_eq!(resolved.default_method().unwrap().name, "c8y-ca");
        assert!(resolved
            .url
            .as_ref()
            .is_some_and(|u| !u.description.is_empty()));
        // its own implied config and (absent) settings stay its own
        assert_eq!(
            resolved.set_config.get("mqtt_service.enabled").unwrap(),
            "true"
        );
        assert!(resolved.settings.is_empty());

        // a derived cloud declaring its own methods keeps them
        let restricted: CloudDescriptor = toml::from_str(
            r#"
cloud = "c8y-restricted"
type = "c8y"
register = [{ name = "certificate", default = true }]
"#,
        )
        .unwrap();
        by_cloud.insert(restricted.cloud.clone(), restricted);
        resolve_derived_clouds(&mut by_cloud);
        assert_eq!(by_cloud["c8y-restricted"].register.len(), 1);
    }

    #[test]
    fn custom_descriptor_parses() {
        let descriptor: CloudDescriptor = toml::from_str(
            r#"
cloud = "thingsboard"
description = "ThingsBoard IoT platform"

[[register]]
name = "token"
default = true
description = "Provision an access token via the Device Provisioning API"
inputs = [
  { name = "provision key", env = "TB_PROVISION_KEY", secret = true },
  { name = "provision secret", env = "TB_PROVISION_SECRET", secret = true },
]
set = { auth_method = "password" }

[[register]]
name = "certificate"
description = "X.509 device certificate registered in ThingsBoard"
set = { auth_method = "certificate" }

[[settings]]
key = "transport.port"
description = "MQTT port"
"#,
        )
        .unwrap();
        assert_eq!(descriptor.default_method().unwrap().name, "token");
        assert_eq!(descriptor.method("token").unwrap().inputs.len(), 2);
    }
}
