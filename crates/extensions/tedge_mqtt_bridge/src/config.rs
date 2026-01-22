use crate::topics::matches_ignore_dollar_prefix;
use crate::topics::TopicConverter;
use ariadne::Color;
use ariadne::Label;
use ariadne::Report;
use ariadne::ReportKind;
use ariadne::Source;
use camino::Utf8Path;
use certificate::parse_root_certificate::create_tls_config;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use rumqttc::valid_filter;
use rumqttc::valid_topic;
use rumqttc::MqttOptions;
use rumqttc::Transport;
use serde::Deserialize;
use serde::Serialize;
use serde_spanned::Spanned;
use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;
use std::str::FromStr;
use tedge_config::models::TemplatesSet;
use tedge_config::tedge_toml::CloudConfig;
use tedge_config::tedge_toml::ConfigNotSet;
use tedge_config::tedge_toml::ParseKeyError;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadError;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::TEdgeConfig;

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct PersistedBridgeConfig {
    local_prefix: Option<Spanned<Template>>,
    remote_prefix: Option<Spanned<Template>>,
    #[serde(rename = "rule", default)]
    rules: Vec<Spanned<StaticBridgeRule>>,
    #[serde(rename = "template_rule", default)]
    template_rules: Vec<Spanned<TemplateBridgeRule>>,
    #[serde(default)]
    r#if: Option<Spanned<Condition>>,
}

#[derive(Debug)]
pub struct ExpandedBridgeRule {
    pub local_prefix: String,
    pub remote_prefix: String,
    pub direction: Direction,
    pub topic: String,
}

fn resolve_prefix(
    per_rule: Option<String>,
    global: &Option<String>,
    prefix_name: &str,
    rule_type: &str,
    span: std::ops::Range<usize>,
) -> Result<String, ExpandError> {
    per_rule
        .or_else(|| global.clone())
        .ok_or_else(|| ExpandError {
            message: format!("Missing '{prefix_name}' for {rule_type}"),
            help: Some(format!(
            "Add '{prefix_name}' to this {rule_type}, or define it globally at the top of the file"
        )),
            span,
        })
}

impl PersistedBridgeConfig {
    pub fn expand(
        &self,
        config: &TEdgeConfig,
        auth_method: AuthMethod,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Vec<ExpandedBridgeRule>, ExpandError> {
        let file_condition = self
            .r#if
            .as_ref()
            .map(|rule| {
                expand_spanned(
                    rule,
                    (config, auth_method),
                    cloud_profile,
                    "Failed to expand global if",
                )
            })
            .transpose()?;
        let  template_disabled = file_condition == Some(false);

        let local_prefix = self
            .local_prefix
            .as_ref()
            .map(|rule| {
                expand_spanned(
                    rule,
                    config,
                    cloud_profile,
                    "Failed to expand global local_prefix",
                )
            })
            .transpose()?;
        let remote_prefix = self
            .remote_prefix
            .as_ref()
            .map(|rule| {
                expand_spanned(
                    rule,
                    config,
                    cloud_profile,
                    "Failed to expand global remote_prefix",
                )
            })
            .transpose()?;

        let mut expanded_rules = Vec::new();
        for spanned_rule in &self.rules {
            let rule = spanned_rule.get_ref();
            let rule_span = spanned_rule.span();
            let rule_condition = rule
                .r#if
                .as_ref()
                .map(|rule| {
                    expand_spanned(
                        rule,
                        (config, auth_method),
                        cloud_profile,
                        "Failed to expand global if",
                    )
                })
                .transpose()?;
            let rule_disabled = template_disabled || rule_condition == Some(false);

            let rule_local_prefix = rule
                .local_prefix
                .as_ref()
                .map(|spanned| {
                    expand_spanned(
                        spanned,
                        config,
                        cloud_profile,
                        "Failed to expand local_prefix",
                    )
                })
                .transpose()?;
            let rule_remote_prefix = rule
                .remote_prefix
                .as_ref()
                .map(|spanned| {
                    expand_spanned(
                        spanned,
                        config,
                        cloud_profile,
                        "Failed to expand remote_prefix",
                    )
                })
                .transpose()?;

            let final_local_prefix = resolve_prefix(
                rule_local_prefix,
                &local_prefix,
                "local_prefix",
                "rule",
                rule_span.clone(),
            )?;
            let final_remote_prefix = resolve_prefix(
                rule_remote_prefix,
                &remote_prefix,
                "remote_prefix",
                "rule",
                rule_span,
            )?;

            let expanded = ExpandedBridgeRule {
                    local_prefix: final_local_prefix,
                    remote_prefix: final_remote_prefix,
                    direction: rule.direction,
                    topic: expand_spanned(
                        &rule.topic,
                        config,
                        cloud_profile,
                        "Failed to expand topic",
                    )?,
                };
            if !rule_disabled {
                expanded_rules.push(expanded);
            }
        }

        for spanned_template in &self.template_rules {
            let template = spanned_template.get_ref();
            let template_span = spanned_template.span();
            let template_condition = template
                .r#if
                .as_ref()
                .map(|rule| {
                    expand_spanned(
                        rule,
                        (config, auth_method),
                        cloud_profile,
                        "Failed to expand global if",
                    )
                })
                .transpose()?;
            let template_rule_disabled = template_disabled || template_condition == Some(false);

            let iterable = expand_spanned(
                &template.r#for,
                config,
                cloud_profile,
                "Failed to expand 'for' reference",
            )?;

            let template_local_prefix = template
                .local_prefix
                .as_ref()
                .map(|spanned| {
                    expand_spanned(
                        spanned,
                        config,
                        cloud_profile,
                        "Failed to expand local_prefix",
                    )
                })
                .transpose()?;
            let template_remote_prefix = template
                .remote_prefix
                .as_ref()
                .map(|spanned| {
                    expand_spanned(
                        spanned,
                        config,
                        cloud_profile,
                        "Failed to expand remote_prefix",
                    )
                })
                .transpose()?;

            let final_local_prefix = resolve_prefix(
                template_local_prefix,
                &local_prefix,
                "local_prefix",
                "template_rule",
                template_span.clone(),
            )?;
            let final_remote_prefix = resolve_prefix(
                template_remote_prefix,
                &remote_prefix,
                "remote_prefix",
                "template_rule",
                template_span,
            )?;

            for topic in iterable.0 {
                let template_config = TemplateConfig {
                    r#for: &topic,
                    tedge: config,
                };

                let expanded = ExpandedBridgeRule {
                    local_prefix: final_local_prefix.clone(),
                    remote_prefix: final_remote_prefix.clone(),
                    direction: template.direction,
                    topic: expand_spanned(
                        &template.topic,
                        template_config,
                        cloud_profile,
                        "Failed to expand topic template",
                    )?,
                };
                if !template_rule_disabled {
expanded_rules.push(expanded);
                }
            }
        }

        Ok(expanded_rules)
    }
}

#[derive(Debug)]
pub struct ExpandError {
    pub message: String,
    pub help: Option<String>,
    pub span: std::ops::Range<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct StaticBridgeRule {
    local_prefix: Option<Spanned<Template>>,
    remote_prefix: Option<Spanned<Template>>,
    direction: Direction,
    topic: Spanned<Template>,
    r#if: Option<Spanned<Condition>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TemplateBridgeRule {
    r#for: Spanned<Iterable>,
    topic: Spanned<TemplateTemplate>,
    local_prefix: Option<Spanned<Template>>,
    remote_prefix: Option<Spanned<Template>>,
    direction: Direction,
    r#if: Option<Spanned<Condition>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Inbound,
    Outbound,
    Bidirectional,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(try_from = "String", into = "String")]
enum Condition {
    AuthMethod(AuthMethod),
    BooleanConfig(ConfigReference<bool>),
}

#[derive(strum::EnumString, strum::Display, Clone, Copy, Debug, PartialEq, Eq)]
#[strum(serialize_all = "snake_case")]
pub enum AuthMethod {
    Certificate,
    Password,
}

impl FromStr for Condition {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_auth_method(s: &str) -> Result<Condition, anyhow::Error> {
            let s = s.strip_prefix("auth_method(")
        .ok_or_else(|| anyhow::anyhow!("Unknown condition. Currently supported conditions: auth_method(...), ${{.some.boolean.config}}"))?;
            let s = s
                .strip_suffix(")")
                .ok_or_else(|| anyhow::anyhow!("Condition must end with ')'"))?;

            let method: AuthMethod = s.parse()?;
            Ok(Condition::AuthMethod(method))
        }

        fn parse_bool_config(s: &str) -> Result<Condition, anyhow::Error> {
            // TODO error handle/better parsing
            let config_ref = s.parse::<ConfigReference<bool>>()?;
            Ok(Condition::BooleanConfig(config_ref))
        }

        parse_bool_config(s).or_else(|_| parse_auth_method(s))
    }
}

impl TryFrom<String> for Condition {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Condition> for String {
    fn from(val: Condition) -> String {
        val.to_string()
    }
}

impl Expandable for Condition {
    type Target = bool;
    type Config<'a> = (&'a TEdgeConfig, AuthMethod);

    fn expand(
        &self,
        config: Self::Config<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        match self {
            Self::AuthMethod(auth_method) => Ok(*auth_method == config.1),
            Self::BooleanConfig(config_ref) => config_ref.expand(config.0, cloud_profile),
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthMethod(method) => write!(f, "auth_method({method})"),
            Self::BooleanConfig(config) => write!(f, "{config}"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct Template(String);

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct TemplateTemplate(String);

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Iterable {
    Config(ConfigReference<TemplatesSet>),
    Literal(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(try_from = "String", into = "String", bound = "")]
struct ConfigReference<Target>(String, PhantomData<Target>);

impl<Target> Clone for ConfigReference<Target> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1)
    }
}

impl<Target> FromStr for ConfigReference<Target> {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .strip_prefix("${.")
            .ok_or_else(|| anyhow::anyhow!("Config variable must start with ${{."))?;
        let s = s
            .strip_suffix("}")
            .ok_or_else(|| anyhow::anyhow!("Config variable must end with }}"))?;
        Ok(Self(s.to_owned(), PhantomData))
    }
}

impl<Target> fmt::Display for ConfigReference<Target> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${{.{}}}", self.0)
    }
}

impl<Target> From<ConfigReference<Target>> for String {
    fn from(val: ConfigReference<Target>) -> String {
        val.to_string()
    }
}

trait Expandable {
    type Target;
    type Config<'a>
    where
        Self: 'a;

    /// Expand the template, returning the result and any error with byte offset
    fn expand(
        &self,
        config: Self::Config<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError>;
}

#[derive(Debug)]
struct TemplateError {
    message: String,
    help: Option<String>,
    /// Byte offset within the template string where the error occurred
    offset: usize,
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TemplateError {}

/// Adjust a span from serde_spanned (which includes quotes) to exclude them,
/// and add an offset for errors within the string content
fn adjust_span_for_error(
    outer_span: std::ops::Range<usize>,
    inner_offset: usize,
) -> std::ops::Range<usize> {
    // serde_spanned includes the quotes, so we add 1 to skip the opening quote
    let content_start = outer_span.start + 1;
    let content_end = outer_span.end.saturating_sub(1).max(content_start);

    // Add the offset within the content
    let error_start = content_start + inner_offset;
    let error_end = content_end;

    error_start..error_end
}

/// Helper to expand a spanned template and convert errors to ExpandError
fn expand_spanned<T: Expandable>(
    spanned: &Spanned<T>,
    config: T::Config<'_>,
    cloud_profile: Option<&ProfileName>,
    context: &str,
) -> Result<T::Target, ExpandError> {
    let span = spanned.span();
    spanned
        .get_ref()
        .expand(config, cloud_profile)
        .map_err(|e| ExpandError {
            message: format!("{context}: {e}"),
            help: e.help,
            span: adjust_span_for_error(span, e.offset),
        })
}

impl Expandable for Iterable {
    type Target = TemplatesSet;
    type Config<'a>
        = &'a TEdgeConfig
    where
        Self: 'a;

    fn expand(
        &self,
        tedge_config: &TEdgeConfig,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        match self {
            Self::Config(config_ref) => config_ref.expand(tedge_config, cloud_profile),
            Self::Literal(values) => Ok(TemplatesSet(values.clone())),
        }
    }
}
impl<Target> Expandable for ConfigReference<Target>
where
    Target: for<'de> serde::Deserialize<'de> + FromStr + std::fmt::Debug,
    Target::Err: std::error::Error + Send + Sync + 'static,
{
    type Target = Target;
    type Config<'a>
        = &'a TEdgeConfig
    where
        Self: 'a;

    fn expand(
        &self,
        config: &TEdgeConfig,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        let key: ReadableKey = self.0.parse().map_err(|e: ParseKeyError| TemplateError {
            message: e.to_string(),
            help: None,
            offset: 0,
        })?;
        let key = if let Some(profile) = cloud_profile {
            // Key might potentially not be a profiled configuration
            // If it isn't, just ignore the profile
            key.clone()
                .try_with_profile(profile.to_owned())
                .unwrap_or(key)
        } else {
            key
        };
        let value = config.read_string(&key).map_err(|e| {
            let (message, help) = if let ReadError::ConfigNotSet(ConfigNotSet { key }) = e {
                (
                    format!("A value for '{key}' is not set"),
                    Some(format!(
                        "A value can be set with `tedge config set {key} <value>`"
                    )),
                )
            } else {
                (e.to_string(), None)
            };
            TemplateError {
                message,
                help,
                offset: 0,
            }
        })?;

        // Try to deserialize as TOML first (for complex types like TemplatesSet)
        // If that fails, fall back to FromStr parsing (for simple string types)
        let deser = toml::de::ValueDeserializer::parse(&value);
        deser
            .and_then(Target::deserialize)
            .or_else(|_| value.parse())
            .map_err(|e: Target::Err| TemplateError {
                message: e.to_string(),
                help: None,
                offset: 0,
            })
    }
}

struct TemplateConfig<'a> {
    tedge: &'a TEdgeConfig,
    r#for: &'a str,
}

/// Expands a tedge.toml config key and return its value
///
/// Expects var_name to start with "." (to match tedge-flows config format),
/// strips it, and reads the config value
fn expand_config_key(
    var_name: &str,
    config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
    offset: usize,
) -> Result<String, TemplateError> {
    let key_str = var_name.strip_prefix(".").ok_or_else(|| TemplateError {
        message: format!("Templated variable must start with '.' (got '{var_name}'))"),
        help: Some(format!("You might have meant '.{var_name}'")),
        offset,
    })?;

    let key: ReadableKey = key_str.parse().map_err(|e: ParseKeyError| TemplateError {
        message: e.to_string(),
        help: None,
        offset,
    })?;

    let key = if let Some(profile) = cloud_profile {
        // Key might potentially not be a profiled configuration
        // If it isn't, just ignore the profile
        key.clone()
            .try_with_profile(profile.to_owned())
            .unwrap_or(key)
    } else {
        key
    };

    config.read_string(&key).map_err(|e| {
        let (message, help) = if let ReadError::ConfigNotSet(ConfigNotSet { key }) = e {
            (
                format!("A value for '{key}' is not set"),
                Some(format!(
                    "A value can be set with `tedge config set {key} <value>`"
                )),
            )
        } else {
            (e.to_string(), None)
        };
        TemplateError {
            message,
            help,
            offset,
        }
    })
}

/// Generic helper to expand template strings with variable substitution
/// The `expand_var` closure is called for each ${...} found, with the variable name and offset
fn expand_template_string(
    template: &str,
    mut expand_var: impl FnMut(&str, usize) -> Result<String, TemplateError>,
) -> Result<String, TemplateError> {
    let mut result = String::new();
    let mut byte_offset = 0;

    for segment in template.split('}') {
        match segment.split_once("${") {
            None => {
                result.push_str(segment);
                byte_offset += segment.len();
            }
            Some((prefix, var_name)) => {
                result.push_str(prefix);
                let var_start = byte_offset + prefix.len();

                let value = expand_var(var_name, var_start)?;
                result.push_str(&value);
                byte_offset += segment.len() + 1; // +1 for the '}'
            }
        }
    }

    Ok(result)
}

impl Expandable for Template {
    type Target = String;
    type Config<'a>
        = &'a TEdgeConfig
    where
        Self: 'a;

    fn expand(
        &self,
        config: Self::Config<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        expand_template_string(&self.0, |var_name, offset| {
            expand_config_key(var_name, config, cloud_profile, offset)
        })
    }
}

impl Expandable for TemplateTemplate {
    type Target = String;
    type Config<'a>
        = TemplateConfig<'a>
    where
        Self: 'a;

    fn expand(
        &self,
        config: TemplateConfig,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        expand_template_string(&self.0, |var_name, offset| {
            if var_name == "@for" {
                Ok(config.r#for.to_string())
            } else {
                expand_config_key(var_name, config.tedge, cloud_profile, offset)
            }
        })
    }
}

impl<Target> TryFrom<String> for ConfigReference<Target> {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

pub fn use_key_and_cert(
    config: &mut MqttOptions,
    cloud_config: &dyn CloudConfig,
) -> anyhow::Result<()> {
    let tls_config = create_tls_config(
        cloud_config.root_cert_path(),
        cloud_config.device_key_path(),
        cloud_config.device_cert_path(),
    )?;
    config.set_transport(Transport::tls_with_config(tls_config.into()));
    Ok(())
}

pub fn use_credentials(
    config: &mut MqttOptions,
    root_cert_path: impl AsRef<Path>,
    username: String,
    password: String,
) -> anyhow::Result<()> {
    let tls_config = create_tls_config_without_client_cert(root_cert_path)?;
    config.set_transport(Transport::tls_with_config(tls_config.into()));
    config.set_credentials(username, password);
    Ok(())
}

#[derive(Default, Debug, Clone)]
pub struct BridgeConfig {
    local_to_remote: Vec<BridgeRule>,
    remote_to_local: Vec<BridgeRule>,
    bidirectional_topics: Vec<(Cow<'static, str>, Cow<'static, str>)>,
}

#[derive(Debug, Clone)]
/// A rule for forwarding MQTT messages from one broker to another
///
/// A rule has three parts, a filter, a prefix to add and a prefix to remove. For instance, the rule
/// `filter: "s/us", prefix_to_remove: "c8y/", prefix_to_add: ""`, will map the topic `c8y/s/us`
/// to `s/us`. The filter can contain wildcards, or be empty (in which case the prefix to remove is
/// the sole local topic that will be remapped).
///
/// ```
/// use tedge_mqtt_bridge::BridgeRule;
///
/// let outgoing_c8y = BridgeRule::try_new("s/us".into(), "c8y/".into(), "".into()).unwrap();
/// assert_eq!(outgoing_c8y.apply("c8y/s/us").unwrap(), "s/us");
///
/// let single_topic = BridgeRule::try_new("".into(), "my/input/topic".into(), "different/output/topic".into()).unwrap();
/// assert_eq!(single_topic.apply("my/input/topic").unwrap(), "different/output/topic");
///
/// let wildcard = BridgeRule::try_new("test/#".into(), "a/".into(), "b/".into()).unwrap();
/// assert_eq!(wildcard.apply("a/test/me").unwrap(), "b/test/me");
/// ```
///
/// This bridge rule logic is based on mosquitto's rule (see the `topic` section of the
/// [mosquitto.conf man page](https://mosquitto.org/man/mosquitto-conf-5.html) for details on what
/// is supported).
pub struct BridgeRule {
    topic_filter: Cow<'static, str>,
    prefix_to_remove: Cow<'static, str>,
    prefix_to_add: Cow<'static, str>,
}

#[derive(Debug, thiserror::Error)]
pub enum InvalidBridgeRule {
    #[error("{0:?} is not a valid MQTT bridge topic prefix as it is missing a trailing slash")]
    MissingTrailingSlash(Cow<'static, str>),

    #[error(
    "{0:?} is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"
    )]
    Empty(BridgeRule),

    #[error("{0:?} is not a valid MQTT bridge topic prefix because it contains '+' or '#'")]
    InvalidTopicPrefix(String),

    #[error("{0:?} is not a valid MQTT bridge topic filter")]
    InvalidTopicFilter(String),

    #[error("Invalid bridge config template")]
    Template,
}

fn validate_topic(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_topic(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicPrefix(topic.to_owned())),
    }
}

fn validate_filter(topic: &str) -> Result<(), InvalidBridgeRule> {
    match valid_filter(topic) {
        true => Ok(()),
        false => Err(InvalidBridgeRule::InvalidTopicFilter(topic.to_owned())),
    }
}

impl BridgeRule {
    pub fn try_new(
        base_topic_filter: Cow<'static, str>,
        prefix_to_remove: Cow<'static, str>,
        prefix_to_add: Cow<'static, str>,
    ) -> Result<Self, InvalidBridgeRule> {
        let filter_is_empty = base_topic_filter.is_empty();
        let mut r = Self {
            topic_filter: prefix_to_remove.clone() + base_topic_filter.clone(),
            prefix_to_remove,
            prefix_to_add,
        };

        validate_topic(&r.prefix_to_add)?;
        validate_topic(&r.prefix_to_remove)?;
        if filter_is_empty {
            if r.prefix_to_add.is_empty() || r.prefix_to_remove.is_empty() {
                r.topic_filter = base_topic_filter;
                Err(InvalidBridgeRule::Empty(r))
            } else {
                Ok(r)
            }
        } else if !(r.prefix_to_remove.ends_with('/') || r.prefix_to_remove.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_remove))
        } else if !(r.prefix_to_add.ends_with('/') || r.prefix_to_add.is_empty()) {
            Err(InvalidBridgeRule::MissingTrailingSlash(r.prefix_to_add))
        } else {
            validate_filter(&base_topic_filter)?;
            Ok(r)
        }
    }

    pub fn apply<'a>(&self, topic: &'a str) -> Option<Cow<'a, str>> {
        matches_ignore_dollar_prefix(topic, &self.topic_filter).then(|| {
            self.prefix_to_add.clone() + topic.strip_prefix(&*self.prefix_to_remove).unwrap()
        })
    }
}

impl BridgeConfig {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn forward_from_local(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.local_to_remote.push(BridgeRule::try_new(
            topic.into(),
            local_prefix.into(),
            remote_prefix.into(),
        )?);
        Ok(())
    }

    pub fn forward_from_remote(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        self.remote_to_local.push(BridgeRule::try_new(
            topic.into(),
            remote_prefix.into(),
            local_prefix.into(),
        )?);
        Ok(())
    }

    /// Forwards the message in both directions, ensuring that an infinite loop is avoided
    ///
    /// Because this method keeps track of the topic so we don't create an infinite loop of messages
    /// this is not equivalent to calling [forward_from_local] and [forward_from_remote] in sequence.
    pub fn forward_bidirectionally(
        &mut self,
        topic: impl Into<Cow<'static, str>>,
        local_prefix: impl Into<Cow<'static, str>>,
        remote_prefix: impl Into<Cow<'static, str>>,
    ) -> Result<(), InvalidBridgeRule> {
        let topic = topic.into();
        let local_prefix = local_prefix.into();
        let remote_prefix = remote_prefix.into();
        self.bidirectional_topics.push((
            local_prefix.clone() + topic.clone(),
            remote_prefix.clone() + topic.clone(),
        ));
        self.forward_from_local(topic.clone(), local_prefix.clone(), remote_prefix.clone())?;
        self.forward_from_remote(topic, local_prefix, remote_prefix)?;
        Ok(())
    }

    pub fn local_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.local_to_remote
            .iter()
            .map(|rule| rule.topic_filter.as_ref())
    }

    pub fn remote_subscriptions(&self) -> impl Iterator<Item = &str> {
        self.remote_to_local.iter().map(|rule| &*rule.topic_filter)
    }

    pub(super) fn converters_and_bidirectional_topic_filters(
        self,
    ) -> [(TopicConverter, Vec<Cow<'static, str>>); 2] {
        let Self {
            local_to_remote,
            remote_to_local,
            bidirectional_topics,
            ..
        } = self;

        let (bidir_local_topics, bidir_remote_topics) = bidirectional_topics.into_iter().unzip();
        [
            (TopicConverter(local_to_remote), bidir_local_topics),
            (TopicConverter(remote_to_local), bidir_remote_topics),
        ]
    }

    pub fn add_rules_from_template(
        &mut self,
        file_path: &Utf8Path,
        toml_template: &str,
        tedge_config: &TEdgeConfig,
        auth_method: AuthMethod,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<(), InvalidBridgeRule> {
        let config: PersistedBridgeConfig = toml::from_str(toml_template).map_err(|e| {
            print_toml_error(file_path.as_str(), toml_template, &e);
            InvalidBridgeRule::Template
        })?;
        let rules = config
            .expand(tedge_config, auth_method, cloud_profile)
            .map_err(|e| {
                print_expansion_error(file_path.as_str(), toml_template, &e);
                InvalidBridgeRule::Template
            })?;
        for rule in rules {
            match rule.direction {
                Direction::Outbound => {
                    self.forward_from_local(rule.topic, rule.local_prefix, rule.remote_prefix)?;
                }
                Direction::Inbound => {
                    self.forward_from_remote(rule.topic, rule.local_prefix, rule.remote_prefix)?;
                }
                Direction::Bidirectional => {
                    self.forward_bidirectionally(
                        rule.topic,
                        rule.local_prefix,
                        rule.remote_prefix,
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn print_toml_error(path: &str, source: &str, error: &toml::de::Error) {
    let span = error.span().unwrap_or(0..0);

    Report::build(ReportKind::Error, (path, span.clone()))
        .with_message("Failed to parse TOML configuration")
        .with_label(
            Label::new((path, span))
                .with_message(error.message())
                .with_color(Color::Red),
        )
        .finish()
        .eprint((path, Source::from(source)))
        .unwrap();
}

fn print_expansion_error(path: &str, source: &str, error: &ExpandError) {
    let mut report = Report::build(ReportKind::Error, (path, error.span.clone()))
        .with_message("Failed to expand bridge configuration")
        .with_label(
            Label::new((path, error.span.clone()))
                .with_message(&error.message)
                .with_color(Color::Red),
        );
    if let Some(help) = &error.help {
        report = report.with_note(help);
    }
    report
        .finish()
        .eprint((path, Source::from(source)))
        .unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;

    mod use_key_and_cert {
        use super::use_key_and_cert;
        use rumqttc::MqttOptions;
        use rumqttc::Transport;
        use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
        use tedge_config::tedge_toml::ProfileName;
        use tedge_config::TEdgeConfig;

        #[tokio::test]
        async fn sets_certs_in_the_provided_mqtt_config() {
            let mut opts = MqttOptions::new("dummy-device", "127.0.0.1", 1883);
            let device_cert = rcgen::generate_simple_self_signed(["dummy-device".into()]).unwrap();
            let c8y_cert = rcgen::generate_simple_self_signed(["dummy-c8y".into()]).unwrap();

            let ttd = tedge_test_utils::fs::TempTedgeDir::new();
            let certs_dir = ttd.path().join("device-certs");
            std::fs::create_dir(&certs_dir).unwrap();
            std::fs::write(
                certs_dir.join("tedge-certificate.pem"),
                device_cert.cert.pem(),
            )
            .unwrap();
            std::fs::write(
                certs_dir.join("tedge-private-key.pem"),
                device_cert.signing_key.serialize_pem(),
            )
            .unwrap();

            let root_cert_path = ttd.path().join("cloud-certs/c8y.pem");
            std::fs::create_dir(root_cert_path.parent().unwrap()).unwrap();
            std::fs::write(&root_cert_path, c8y_cert.cert.pem()).unwrap();
            std::fs::write(ttd.path().join("tedge.toml"), "c8y.url = \"example.com\"").unwrap();
            let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
            let c8y_config = tedge_config
                .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
                .unwrap();

            use_key_and_cert(&mut opts, &c8y_config).unwrap();

            let Transport::Tls(tls) = opts.transport() else {
                panic!("Transport should be type TLS")
            };
            let rumqttc::TlsConfiguration::Rustls(config) = tls else {
                panic!("{tls:?} is not Rustls")
            };
            assert!(
                config.client_auth_cert_resolver.has_certs(),
                "Should have certs"
            );
        }
    }

    mod bridge_rule {
        use super::*;

        #[test]
        fn forward_topics_without_any_prefixes() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()))
        }

        #[test]
        fn forwards_wildcard_topics() {
            let rule = BridgeRule::try_new("a/#".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("a/topic".into()));
        }

        #[test]
        fn does_not_forward_non_matching_topics() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert_eq!(rule.apply("different/topic"), None)
        }

        #[test]
        fn removes_local_prefix() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("topic".into()));
        }

        #[test]
        fn prepends_remote_prefix() {
            // TODO maybe debug warn if topic filter begins with prefix to remove
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap();
            assert_eq!(rule.apply("a/topic"), Some("b/topic".into()));
        }

        #[test]
        fn does_not_clone_if_topic_is_unchanged() {
            let rule = BridgeRule::try_new("a/topic".into(), "".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn does_not_clone_if_prefix_is_removed_but_not_added() {
            let rule = BridgeRule::try_new("topic".into(), "a/".into(), "".into()).unwrap();
            assert!(matches!(rule.apply("a/topic"), Some(Cow::Borrowed(_))))
        }

        #[test]
        fn matches_topics_with_dollar_prefix() {
            let rule =
                BridgeRule::try_new("twin/res/#".into(), "$iothub/".into(), "az/".into()).unwrap();
            assert_eq!(
                rule.apply("$iothub/twin/res/200/?$rid=1"),
                Some("az/twin/res/200/?$rid=1".into())
            )
        }

        #[test]
        fn forwards_unfiltered_topic() {
            let cloud_topic = "thinedge/devices/my-device/test-connection";
            let rule =
                BridgeRule::try_new("".into(), "aws/test-connection".into(), cloud_topic.into())
                    .unwrap();
            assert_eq!(rule.apply("aws/test-connection"), Some(cloud_topic.into()))
        }

        #[test]
        fn allows_empty_input_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "".into(), "output/".into()).unwrap();
            assert_eq!(rule.apply("test/me"), Some("output/test/me".into()));
        }

        #[test]
        fn allows_empty_output_prefix() {
            let rule = BridgeRule::try_new("test/#".into(), "input/".into(), "".into()).unwrap();
            assert_eq!(rule.apply("input/test/me"), Some("test/me".into()));
        }

        #[test]
        fn rejects_invalid_input_prefix() {
            let err = BridgeRule::try_new("test/#".into(), "wildcard/#".into(), "output/".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/#\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_invalid_output_prefix() {
            let err = BridgeRule::try_new("test/#".into(), "input/".into(), "wildcard/+".into())
                .unwrap_err();
            assert_eq!(err.to_string(), "\"wildcard/+\" is not a valid MQTT bridge topic prefix because it contains '+' or '#'");
        }

        #[test]
        fn rejects_input_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input".into(), "output/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"input\" is not a valid MQTT bridge topic prefix as it is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_output_prefix_missing_trailing_slash() {
            let err =
                BridgeRule::try_new("test/#".into(), "input/".into(), "output".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"output\" is not a valid MQTT bridge topic prefix as it is missing a trailing slash"
            );
        }

        #[test]
        fn rejects_empty_input_prefix_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "".into(), "a/".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "", prefix_to_add: "a/" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }

        #[test]
        fn rejects_empty_output_prefix_with_empty_filter() {
            let err = BridgeRule::try_new("".into(), "a/".into(), "".into()).unwrap_err();
            assert_eq!(
                err.to_string(),
                r#"BridgeRule { topic_filter: "", prefix_to_remove: "a/", prefix_to_add: "" } is not a valid rule, at least one of the topic filter or both prefixes must be non-empty"#
            )
        }
    }

    mod topic_converter {
        use super::*;
        #[test]
        fn applies_matching_topic() {
            let converter = TopicConverter(vec![BridgeRule::try_new(
                "topic".into(),
                "a/".into(),
                "b/".into(),
            )
            .unwrap()]);
            assert_eq!(converter.convert_topic("a/topic"), Some("b/topic".into()))
        }

        #[test]
        fn applies_first_matching_topic_if_multiple_are_provided() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "a/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), Some("b/topic".into()));
        }

        #[test]
        fn does_not_apply_non_matching_topics() {
            let converter = TopicConverter(vec![
                BridgeRule::try_new("topic".into(), "x/".into(), "b/".into()).unwrap(),
                BridgeRule::try_new("#".into(), "a/".into(), "c/".into()).unwrap(),
            ]);
            assert_eq!(converter.convert_topic("a/topic"), Some("c/topic".into()));
        }
    }

    mod validate_filter {
        use crate::config::validate_filter;

        #[test]
        fn accepts_wildcard_filters() {
            validate_filter("test/#").unwrap();
        }

        #[test]
        fn accepts_single_topics() {
            validate_filter("valid/topic").unwrap();
        }

        #[test]
        fn includes_supplied_value_in_error_message() {
            let err = validate_filter("invalid/#/filter").unwrap_err();
            assert_eq!(
                err.to_string(),
                "\"invalid/#/filter\" is not a valid MQTT bridge topic filter"
            );
        }
    }

    mod persisted_bridge_config {
        use super::*;

        #[test]
        fn deserializes_basic_config_with_static_rules() {
            let toml = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"

                [[rule]]
                topic = "test/topic"
                direction = "inbound"
            "#;

            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            assert_eq!(config.rules.len(), 1);
            assert_eq!(config.template_rules.len(), 0);
        }

        #[test]
        fn deserializes_config_with_template_rules() {
            let toml = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"

                [[template_rule]]
                for = "${.c8y.topics}"
                topic = "${@for}"
                direction = "outbound"
            "#;

            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            assert_eq!(config.rules.len(), 0);
            assert_eq!(config.template_rules.len(), 1);
        }

        #[test]
        fn deserializes_config_with_mixed_rules() {
            let toml = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"

                [[rule]]
                topic = "test/topic"
                direction = "inbound"

                [[template_rule]]
                for = "${.c8y.topics}"
                topic = "${@for}"
                direction = "outbound"
            "#;

            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            assert_eq!(config.rules.len(), 1);
            assert_eq!(config.template_rules.len(), 1);
        }

        #[test]
        fn deserializes_rule_with_per_rule_prefixes() {
            let toml = r#"
                [[rule]]
                local_prefix = "override-local/"
                remote_prefix = "override-remote/"
                topic = "test/topic"
                direction = "bidirectional"
            "#;

            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            assert_eq!(config.rules.len(), 1);
        }

        #[test]
        fn deserializes_all_direction_types() {
            let toml = r#"
                [[rule]]
                topic = "inbound/topic"
                direction = "inbound"

                [[rule]]
                topic = "outbound/topic"
                direction = "outbound"

                [[rule]]
                topic = "bidirectional/topic"
                direction = "bidirectional"
            "#;

            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            assert_eq!(config.rules.len(), 3);
            assert!(matches!(
                config.rules[0].get_ref().direction,
                Direction::Inbound
            ));
            assert!(matches!(
                config.rules[1].get_ref().direction,
                Direction::Outbound
            ));
            assert!(matches!(
                config.rules[2].get_ref().direction,
                Direction::Bidirectional
            ));
        }

        #[test]
        fn rejects_unknown_fields() {
            let toml = r#"
                local_prefix = "local/"
                remote_prefix = "remote/"
                unknown_field = "value"

                [[rule]]
                topic = "test/topic"
                direction = "inbound"
            "#;

            let result: Result<PersistedBridgeConfig, _> = toml::from_str(toml);
            assert!(result.is_err());
        }

        #[test]
        fn rejects_rule_with_unknown_fields() {
            let toml = r#"
                [[rule]]
                topic = "test/topic"
                direction = "inbound"
                unknown_field = "value"
            "#;

            let result: Result<PersistedBridgeConfig, _> = toml::from_str(toml);
            assert!(result.is_err());
        }

        #[test]
        fn config_reference_parses_valid_reference() {
            let reference = "${.c8y.topics.e}";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            assert!(result.is_ok());
        }

        #[test]
        fn config_reference_rejects_missing_prefix() {
            let reference = "c8y.topics.e}";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("must start with ${."));
        }

        #[test]
        fn config_reference_rejects_missing_suffix() {
            let reference = "${.c8y.mqtt_service.topics";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("must end with }"));
        }

        #[test]
        fn config_reference_expands_to_templates_set() {
            let reference: ConfigReference<TemplatesSet> =
                "${.c8y.smartrest.templates}".parse().unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
[c8y]
smartrest.templates = ["template1", "template2", "template3"]
"#,
            );

            let result = reference.expand(&tedge_config, None).unwrap();
            assert_eq!(result.0, vec!["template1", "template2", "template3"]);
        }

        #[test]
        fn config_reference_expands_string_value() {
            let reference: ConfigReference<String> = "${.c8y.bridge.topic_prefix}".parse().unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
[c8y.bridge]
topic_prefix = "changed"
"#,
            );

            let result = reference.expand(&tedge_config, None).unwrap();
            assert_eq!(result, "changed");
        }

        #[test]
        fn config_reference_is_profile_aware() {
            let reference: ConfigReference<String> = "${.c8y.bridge.topic_prefix}".parse().unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
[c8y.profiles.new.bridge]
topic_prefix = "c8y-new"
"#,
            );

            let result = reference
                .expand(&tedge_config, Some(&"new".parse().unwrap()))
                .unwrap();
            assert_eq!(result, "c8y-new");
        }

        #[test]
        fn span_information_includes_quotes() {
            let toml = r#"local_prefix = "te/""#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let span = config.local_prefix.unwrap().span();

            // serde_spanned includes the quotes in the span
            // "local_prefix = "te/""
            //                ^^^^^ this part (bytes 15..20)
            assert_eq!(span, 15..20, "Span includes quotes");
            assert_eq!(&toml[span.clone()], "\"te/\"");
        }

        #[test]
        fn span_information_for_topic_field() {
            let toml = r#"
[[rule]]
topic = "test/topic"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let span = config.rules[0].get_ref().topic.span();

            // serde_spanned includes the quotes
            let topic_str = &toml[span.clone()];
            assert_eq!(topic_str, "\"test/topic\"", "Span includes quotes");
        }

        #[test]
        fn span_information_for_config_reference() {
            let toml = r#"
[[template_rule]]
for = "${.c8y.topics.e}"
topic = "te/device/main///e/${@for}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let span = config.template_rules[0].get_ref().r#for.span();

            // serde_spanned includes the quotes
            let reference_str = &toml[span.clone()];
            assert_eq!(
                reference_str, "\"${.c8y.topics.e}\"",
                "Span includes quotes"
            );
        }

        #[test]
        fn template_error_offset_points_to_variable() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[rule]]
topic = "prefix/${.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let err = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            // The error span should point to the ${.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn template_errors_are_detected_even_if_template_is_disabled() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""
if = "${.c8y.mqtt_service.enabled}"

[[rule]]
topic = "prefix/${.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let err = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            // The error span should point to the ${.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn template_errors_are_detected_even_if_rule_is_disabled() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[rule]]
if = "${.c8y.mqtt_service.enabled}"
topic = "prefix/${.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let err = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            // The error span should point to the ${.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn template_errors_are_detected_even_if_template_rule_is_disabled() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[template_rule]]
for = ['a', 'b']
if = "${.c8y.mqtt_service.enabled}"
topic = "${@for}/${.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let err = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            // The error span should point to the ${.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn template_rules_can_expand_config_templatesets() {
            let toml = r#"
local_prefix = "${.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = "${.c8y.smartrest.templates}"
topic = "s/dc/${@for}"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.smartrest.templates = ["a", "b"]
            "#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(expanded.len(), 2);
            assert_eq!(expanded[0].topic, "s/dc/a");
            assert_eq!(expanded[1].topic, "s/dc/b");
        }

        #[test]
        fn template_rules_can_expand_literal_arrays() {
            let toml = r#"
local_prefix = "${.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = ['s', 't', 'q', 'c']
topic = "${@for}/us/#"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.smartrest.templates = ["a", "b"]
            "#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(expanded.len(), 4);
            assert_eq!(expanded[0].topic, "s/us/#");
            assert_eq!(expanded[1].topic, "t/us/#");
            assert_eq!(expanded[2].topic, "q/us/#");
            assert_eq!(expanded[3].topic, "c/us/#");
        }

        #[test]
        fn multiple_variables_in_template() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[rule]]
topic = "start/${.first}/middle/${.second}/end"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            // Save the span before we consume config
            let topic_span = config.rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let err = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            // The error should be about the first variable
            assert!(
                err.message.contains("first"),
                "Error message should mention 'first': {}",
                err.message
            );

            // And the span should point somewhere in the topic string
            assert!(
                err.span.start >= topic_span.start && err.span.end <= topic_span.end,
                "Error span {:?} should be within topic span {:?}",
                err.span,
                topic_span
            );
        }

        #[test]
        fn expands_template_rules_with_smartrest_templates() {
            let toml = r#"
local_prefix = "c8y/"
remote_prefix = ""

[[template_rule]]
for = "${.c8y.smartrest.templates}"
topic = "s/uc/${@for}"
direction = "outbound"

[[template_rule]]
for = "${.c8y.smartrest.templates}"
topic = "s/dc/${@for}"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
[c8y]
smartrest.templates = ["template1", "template2", "template3"]
"#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            // Should create 6 rules: 3 outbound + 3 inbound
            assert_eq!(expanded.len(), 6);

            // Check outbound rules (s/uc/${@for})
            assert_eq!(expanded[0].topic, "s/uc/template1");
            assert_eq!(expanded[0].local_prefix, "c8y/");
            assert_eq!(expanded[0].remote_prefix, "");
            assert!(matches!(expanded[0].direction, Direction::Outbound));

            assert_eq!(expanded[1].topic, "s/uc/template2");
            assert_eq!(expanded[2].topic, "s/uc/template3");

            // Check inbound rules (s/dc/${@for})
            assert_eq!(expanded[3].topic, "s/dc/template1");
            assert_eq!(expanded[3].local_prefix, "c8y/");
            assert_eq!(expanded[3].remote_prefix, "");
            assert!(matches!(expanded[3].direction, Direction::Inbound));

            assert_eq!(expanded[4].topic, "s/dc/template2");
            assert_eq!(expanded[5].topic, "s/dc/template3");
        }

        #[test]
        fn templates_are_profile_aware() {
            let toml = r#"
local_prefix = "${.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
topic = "s/us"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
[c8y.profiles.test]
bridge.topic_prefix = "test"
"#,
            );

            let expanded = config
                .expand(
                    &tedge_config,
                    AuthMethod::Certificate,
                    Some(&"test".parse().unwrap()),
                )
                .unwrap();

            assert_eq!(expanded.len(), 1);

            assert_eq!(expanded[0].topic, "s/us");
            assert_eq!(expanded[0].local_prefix, "test/");
            assert_eq!(expanded[0].remote_prefix, "");
            assert!(matches!(expanded[0].direction, Direction::Outbound));
        }

        #[test]
        fn entire_templates_can_be_conditionally_disabled() {
            let toml = r##"
if = "${.c8y.mqtt_service.enabled}"
remote_prefix = ""

[[rule]]
local_prefix = "${.c8y.bridge.topic_prefix}/mqtt/out"
topic = "#"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.mqtt_service.enabled = false
"#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(expanded.len(), 0);
        }

        #[test]
        fn entire_templates_can_be_conditionally_enabled() {
            let toml = r##"
if = "${.c8y.mqtt_service.enabled}"
remote_prefix = ""

[[rule]]
local_prefix = "${.c8y.bridge.topic_prefix}/mqtt/out"
topic = "#"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.mqtt_service.enabled = true
"#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(expanded.len(), 1);
        }

        #[test]
        fn rules_can_be_conditionally_enabled() {
            let toml = r##"
local_prefix = "${.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
if = "auth_method(password)"
topic = "s/ut/#"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let with_certificate = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(with_certificate.len(), 0);

            let with_password = config
                .expand(&tedge_config, AuthMethod::Password, None)
                .unwrap();

            assert_eq!(with_password.len(), 1);
        }

        #[test]
        fn template_rules_can_be_conditionally_enabled() {
            let toml = r##"
local_prefix = "${.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
if = "auth_method(certificate)"
for = ['s', 't', 'q', 'c']
topic = "${@for}/us/#"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let with_certificate = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap();

            assert_eq!(with_certificate.len(), 4);

            let with_password = config
                .expand(&tedge_config, AuthMethod::Password, None)
                .unwrap();

            assert_eq!(with_password.len(), 0);
        }
    }
}
