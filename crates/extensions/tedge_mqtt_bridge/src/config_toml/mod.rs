mod parse_condition;
use serde::Deserialize;
use serde::Serialize;
use serde_spanned::Spanned;
use std::fmt;
use std::marker::PhantomData;
use std::str::FromStr;
use tedge_config::models::TemplatesSet;
use tedge_config::tedge_toml::ConfigNotSet;
use tedge_config::tedge_toml::ParseKeyError;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::tedge_toml::ReadError;
use tedge_config::tedge_toml::ReadableKey;
use tedge_config::TEdgeConfig;

use parse_condition::parse_condition_with_error;

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
    r#if: Option<Spanned<String>>,
}

#[derive(Debug)]
pub struct ExpandedBridgeRule {
    pub local_prefix: String,
    pub remote_prefix: String,
    pub direction: Direction,
    pub topic: String,
}

fn resolve_prefix(
    per_rule: PrefixExpansionState,
    global: &PrefixExpansionState,
    prefix_name: &str,
    rule_type: &str,
    span: std::ops::Range<usize>,
) -> Result<String, Option<ExpandError>> {
    let expanded = per_rule.or_else(|| global.clone());
    match expanded {
        PrefixExpansionState::Expanded(expanded) => Ok(expanded),
        // We don't have the prefix because it failed to parse, don't generate another error here
        PrefixExpansionState::Error => Err(None),
        PrefixExpansionState::NotDefined => Err(Some(ExpandError {
            message: format!("Missing '{prefix_name}' for {rule_type}"),
            help: Some(format!(
            "Add '{prefix_name}' to this {rule_type}, or define it globally at the top of the file"
        )),
            span,
        })),
    }
}

#[derive(Clone, Debug)]
enum PrefixExpansionState {
    NotDefined,
    Error,
    Expanded(String),
}

impl PrefixExpansionState {
    fn or_else(self, f: impl FnOnce() -> Self) -> Self {
        match self {
            Self::Expanded(s) => Self::Expanded(s),
            Self::NotDefined => f(),
            Self::Error => {
                let fallback = f();
                if let Self::NotDefined = fallback {
                    Self::Error
                } else {
                    fallback
                }
            }
        }
    }
}

impl PersistedBridgeConfig {
    pub fn expand(
        &self,
        config: &TEdgeConfig,
        auth_method: AuthMethod,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Vec<ExpandedBridgeRule>, Vec<ExpandError>> {
        let expand_prefix = |prefix: &Option<_>, name| match prefix.as_ref() {
            Some(prefix) => expand_spanned(
                prefix,
                config,
                cloud_profile,
                &format!("Failed to expand {name}"),
            )
            .map(PrefixExpansionState::Expanded),
            None => Ok(PrefixExpansionState::NotDefined),
        };
        let mut errors = Vec::new();
        let expand_condition = |condition: Option<&Spanned<String>>, context: &str| {
            condition
                .map(|s| {
                    let rule: Spanned<Condition> = parse_condition_with_error(s)?;
                    expand_spanned(
                        &rule,
                        (config, auth_method),
                        cloud_profile,
                        &format!("Failed to expand {context} condition"),
                    )
                    .map_err(|err| vec![err])
                })
                .transpose()
        };

        let file_condition =
            expand_condition(self.r#if.as_ref(), "global").unwrap_or_else(|mut e| {
                errors.append(&mut e);
                None
            });
        let template_disabled = file_condition == Some(false);

        let local_prefix =
            expand_prefix(&self.local_prefix, "local_prefix").unwrap_or_else(|e| {
                errors.push(e);
                PrefixExpansionState::Error
            });
        let remote_prefix = expand_prefix(&self.remote_prefix, "remote_prefix")
            .unwrap_or_else(|e| {
                errors.push(e);
                PrefixExpansionState::Error
            });

        let mut expanded_rules = Vec::new();
        for spanned_rule in &self.rules {
            let rule = spanned_rule.get_ref();
            let rule_span = spanned_rule.span();
            let rule_condition = expand_condition(rule.r#if.as_ref(), "rule-specific")
                .unwrap_or_else(|mut e| {
                    errors.append(&mut e);
                    None
                });
            let rule_disabled = template_disabled || rule_condition == Some(false);

            let rule_local_prefix = expand_prefix(&rule.local_prefix, "local_prefix")
                .unwrap_or_else(|e| {
                    errors.push(e);
                    PrefixExpansionState::Error
                });
            let rule_remote_prefix = expand_prefix(&rule.remote_prefix, "remote_prefix")
                .unwrap_or_else(|e| {
                    errors.push(e);
                    PrefixExpansionState::Error
                });

            let final_local_prefix = resolve_prefix(
                rule_local_prefix,
                &local_prefix,
                "local_prefix",
                "rule",
                rule_span.clone(),
            )
            .unwrap_or_else(|e| {
                if let Some(e) = e {
                    errors.push(e);
                }
                String::new()
            });
            let final_remote_prefix = resolve_prefix(
                rule_remote_prefix,
                &remote_prefix,
                "remote_prefix",
                "rule",
                rule_span,
            )
            .unwrap_or_else(|e| {
                if let Some(e) = e {
                    errors.push(e);
                }
                String::new()
            });

            let expanded = ExpandedBridgeRule {
                local_prefix: final_local_prefix,
                remote_prefix: final_remote_prefix,
                direction: rule.direction,
                topic: expand_spanned(&rule.topic, config, cloud_profile, "Failed to expand topic")
                    .unwrap_or_else(|e| {
                        errors.push(e);
                        String::new()
                    }),
            };
            if !rule_disabled {
                expanded_rules.push(expanded);
            }
        }

        'template_rules: for spanned_template in &self.template_rules {
            let error_count = errors.len();
            let template = spanned_template.get_ref();
            let template_span = spanned_template.span();
            let template_condition = expand_condition(template.r#if.as_ref(), "rule-specific")
                .unwrap_or_else(|mut e| {
                    errors.append(&mut e);
                    None
                });
            let template_rule_disabled = template_disabled || template_condition == Some(false);

            let (iterable, ident) = expand_spanned(
                &template.r#for,
                config,
                cloud_profile,
                "Failed to expand 'for' reference",
            )
            .unwrap_or_else(|e| {
                errors.push(e);
                <_>::default()
            });

            let template_local_prefix =
                expand_prefix(&template.local_prefix, "local_prefix").unwrap_or_else(
                    |e| {
                        errors.push(e);
                        PrefixExpansionState::Error
                    },
                );
            let template_remote_prefix =
                expand_prefix(&template.remote_prefix, "remote_prefix").unwrap_or_else(
                    |e| {
                        errors.push(e);
                        PrefixExpansionState::Error
                    },
                );

            let final_local_prefix = resolve_prefix(
                template_local_prefix,
                &local_prefix,
                "local_prefix",
                "template_rule",
                template_span.clone(),
            )
            .unwrap_or_else(|e| {
                if let Some(e) = e {
                    errors.push(e);
                }
                <_>::default()
            });
            let final_remote_prefix = resolve_prefix(
                template_remote_prefix,
                &remote_prefix,
                "remote_prefix",
                "template_rule",
                template_span,
            )
            .unwrap_or_else(|e| {
                if let Some(e) = e {
                    errors.push(e);
                }
                <_>::default()
            });

            if iterable.0.is_empty() {
                let template_config = TemplateConfig {
                    r#for: "",
                    for_ident: &ident,
                    tedge: config,
                };
                // Verify the topic is valid even if there are no rules to generate
                expand_spanned(
                    &template.topic,
                    template_config,
                    cloud_profile,
                    "Failed to expand topic template",
                )
                .unwrap_or_else(|e| {
                    errors.push(e);
                    <_>::default()
                });
            }

            for topic in iterable.0 {
                let template_config = TemplateConfig {
                    r#for: &topic,
                    for_ident: &ident,
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
                    )
                    .unwrap_or_else(|e| {
                        errors.push(e);
                        <_>::default()
                    }),
                };

                if errors.len() > error_count {
                    // We've already registered an error for this template continue to the next
                    continue 'template_rules;
                }

                if !template_rule_disabled {
                    expanded_rules.push(expanded);
                }
            }
        }

        if errors.is_empty() {
            Ok(expanded_rules)
        } else {
            Err(errors)
        }
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
    r#if: Option<Spanned<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TemplateBridgeRule {
    r#for: Spanned<IterationRule>,
    topic: Spanned<TemplateTemplate>,
    local_prefix: Option<Spanned<Template>>,
    remote_prefix: Option<Spanned<Template>>,
    direction: Direction,
    r#if: Option<Spanned<String>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Inbound,
    Outbound,
    Bidirectional,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Condition {
    AuthMethod(AuthMethod),
    IsTrue(ConfigReference<bool>),
}

#[derive(
    //strum::EnumString,
    strum::Display,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
)]
#[strum(serialize_all = "snake_case")]
pub enum AuthMethod {
    Certificate,
    Password,
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
            Self::IsTrue(config_ref) => config_ref.expand(config.0, cloud_profile),
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthMethod(method) => write!(f, "auth_method({method})"),
            Self::IsTrue(config) => write!(f, "{config}"),
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
struct IterationRule {
    // TODO ensure this doesn't contain `.`
    item: Spanned<String>,
    r#in: Spanned<Iterable>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Iterable {
    Config(ConfigReference<TemplatesSet>),
    Literal(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(try_from = "String", into = "String", bound = "")]
pub(crate) struct ConfigReference<Target>(pub(crate) String, pub(crate) PhantomData<Target>);

impl<Target> Clone for ConfigReference<Target> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1)
    }
}

impl<Target> FromStr for ConfigReference<Target> {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .strip_prefix("${config.")
            .ok_or_else(|| anyhow::anyhow!("Config variable must start with ${{config."))?;
        let s = s
            .strip_suffix("}")
            .ok_or_else(|| anyhow::anyhow!("Config variable must end with }}"))?;
        Ok(Self(s.to_owned(), PhantomData))
    }
}

impl<Target> fmt::Display for ConfigReference<Target> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "${{config.{}}}", self.0)
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

impl Expandable for IterationRule {
    type Target = (TemplatesSet, String);
    type Config<'a>
        = &'a TEdgeConfig
    where
        Self: 'a;

    fn expand(
        &self,
        tedge_config: &TEdgeConfig,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, TemplateError> {
        Ok((
            self.r#in.get_ref().expand(tedge_config, cloud_profile)?,
            self.item.get_ref().clone(),
        ))
    }
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
    for_ident: &'a str,
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
    let key_str = var_name
        .strip_prefix("config.")
        .ok_or_else(|| TemplateError {
            message: format!("Templated variable must start with 'config.' (got '{var_name}'))"),
            help: Some(format!("You might have meant 'config.{var_name}'")),
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
        // TODO warn if there is no reference to the iterated variable
        // TODO maybe add warnings if there are duplicate rules
        expand_template_string(&self.0, |var_name, offset| {
            if var_name == config.for_ident {
                Ok(config.r#for.to_string())
            } else if var_name.contains(".") {
                expand_config_key(var_name, config.tedge, cloud_profile, offset)
            } else {
                Err(TemplateError {
                    message: "Unknown variable".to_owned(),
                    help: Some(format!(
                        "You might have meant '{}' or 'config.{}'",
                        config.for_ident, var_name
                    )),
                    offset,
                })
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

#[cfg(test)]
mod tests {
    use super::*;

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
                for = { item = "topic", in = "${config.c8y.topics}" }
                topic = "${topic}"
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
                for = { item = "topic", in = "${config.c8y.topics}" }
                topic = "${topic}"
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
            let reference = "${config.c8y.topics.e}";
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
                .contains("must start with ${config."));
        }

        #[test]
        fn config_reference_rejects_missing_suffix() {
            let reference = "${config.c8y.mqtt_service.topics";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("must end with }"));
        }

        #[test]
        fn config_reference_expands_to_templates_set() {
            let reference: ConfigReference<TemplatesSet> =
                "${config.c8y.smartrest.templates}".parse().unwrap();
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
            let reference: ConfigReference<String> =
                "${config.c8y.bridge.topic_prefix}".parse().unwrap();
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
            let reference: ConfigReference<String> =
                "${config.c8y.bridge.topic_prefix}".parse().unwrap();
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
for = { item = "suffix", in = "${config.c8y.topics.e}" }
topic = "te/device/main///e/${suffix}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let span = config.template_rules[0]
                .get_ref()
                .r#for
                .get_ref()
                .r#in
                .span();

            // serde_spanned includes the quotes
            let reference_str = &toml[span.clone()];
            assert_eq!(
                reference_str, "\"${config.c8y.topics.e}\"",
                "Span includes quotes"
            );
        }

        #[test]
        fn template_error_offset_points_to_variable() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[rule]]
topic = "prefix/${config.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1);
            let err = &errs[0];

            // The error span should point to the ${config.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${config.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn template_errors_are_detected_even_if_template_is_disabled() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""
if = "${config.c8y.mqtt_service.enabled}"

[[rule]]
topic = "prefix/${config.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1);
            let err = &errs[0];

            // The error span should point to the ${config.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${config.invalid"),
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
if = "${config.c8y.mqtt_service.enabled}"
topic = "prefix/${config.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1);
            let err = &errs[0];

            // The error span should point to the ${config.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${config.invalid"),
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
for = { item = "prefix", in = ['a', 'b'] }
if = "${config.c8y.mqtt_service.enabled}"
topic = "${prefix}/${config.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1, "There should be precisely 1 error, got: {errs:?}");
            let err = &errs[0];

            // The error span should point to the ${config.invalid.key} part within the topic string
            let error_text = &toml[err.span.clone()];
            assert!(
                error_text.contains("invalid.key") || error_text.starts_with("${config.invalid"),
                "Error span should point to the problematic template variable, got: {:?}",
                error_text
            );
        }

        #[test]
        fn local_prefix_parse_failure_does_not_cause_spillover_error() {
            let templates = [
                r#"
                
[[rule]]
local_prefix = "${"
                
                "#
                .trim(),
                r#"
                
[[template_rule]]
local_prefix = "${"
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
                
                "#
                .trim(),
                r#"
                
local_prefix = "${"
[[rule]]
                
                "#
                .trim(),
                r#"
                
local_prefix = "${"
[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
                
                "#
                .trim(),
            ];
            for template_start in templates {
                let toml = format!(
                    r#"
{template_start}
remote_prefix = ""
topic = "${{config.c8y.bridge.topic_prefix}}/something/"
direction = "inbound"
"#
                );
                let config: PersistedBridgeConfig = toml::from_str(&toml).unwrap();
                let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

                let errs = config
                    .expand(&tedge_config, AuthMethod::Certificate, None)
                    .unwrap_err();

                assert_eq!(
                    errs.len(),
                    1,
                    "Expected 1 error, got {}. Input toml below:\n\n{toml}",
                    errs.len()
                );
                let err = &errs[0];

                assert!(
                err.message.contains("Failed to expand local_prefix"),
                "Error message should be about non-parsed prefix, not about a missing one, got: {:?}",
                err.message
            );
            }
        }

        #[test]
        fn template_rules_can_expand_config_templatesets() {
            let toml = r#"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
topic = "s/dc/${template}"
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
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = { item = "mode", in = ['s', 't', 'q', 'c'] }
topic = "${mode}/us/#"
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
topic = "start/${config.first}/middle/${config.second}/end"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            // Save the span before we consume config
            let topic_span = config.rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1);
            let err = &errs[0];

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
        fn template_rule_detects_errors_even_if_not_generating_values() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
topic = "${config.something.unknown}/${template}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.smartrest.templates = []");

            // Save the span before we consume config
            let topic_span = config.template_rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1);
            let err = &errs[0];

            assert!(
                err.message.contains("something.unknown"),
                "Error message should mention 'something.unknown': {}",
                err.message
            );

            assert!(
                err.span.start >= topic_span.start && err.span.end <= topic_span.end,
                "Error span {:?} should be within topic span {:?}",
                err.span,
                topic_span
            );
        }

        #[test]
        fn errors_in_multiple_parts_of_template_are_detected_simultaneously() {
            let toml = r#"
local_prefix = ""
remote_prefix = ""

[[rule]]
topic = "${config.first}"
direction = "inbound"

[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
topic = "${config.second}/${template}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            // Save the span before we consume config
            let topic_span = config.rules[0].get_ref().topic.span();
            let template_topic_span = config.template_rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None)
                .unwrap_err();

            assert_eq!(errs.len(), 2);
            let first = &errs[0];

            // The error should be about the first variable
            assert!(
                first.message.contains("first"),
                "Error message should mention 'first': {}",
                first.message
            );

            // And the span should point somewhere in the topic string
            assert!(
                first.span.start >= topic_span.start && first.span.end <= topic_span.end,
                "Error span {:?} should be within topic span {:?}",
                first.span,
                topic_span
            );

            let second = &errs[1];
            assert!(
                second.message.contains("second"),
                "Error message should mention 'second': {}",
                second.message
            );

            assert!(
                second.span.start >= template_topic_span.start
                    && second.span.end <= template_topic_span.end,
                "Error span {:?} should be within topic span {:?}",
                second.span,
                template_topic_span
            );
        }

        #[test]
        fn expands_template_rules_with_smartrest_templates() {
            let toml = r#"
local_prefix = "c8y/"
remote_prefix = ""

[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
topic = "s/uc/${template}"
direction = "outbound"

[[template_rule]]
for = { item = "template", in = "${config.c8y.smartrest.templates}" }
topic = "s/dc/${template}"
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
local_prefix = "${config.c8y.bridge.topic_prefix}/"
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
if = "${config.c8y.mqtt_service.enabled}"
remote_prefix = ""

[[rule]]
local_prefix = "${config.c8y.bridge.topic_prefix}/mqtt/out"
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
if = "${config.c8y.mqtt_service.enabled}"
remote_prefix = ""

[[rule]]
local_prefix = "${config.c8y.bridge.topic_prefix}/mqtt/out"
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
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
if = "${connection.auth_method} == 'password'"
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
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
if = "${connection.auth_method} == 'certificate'"
for = { item = "mode", in = ['s', 't', 'q', 'c'] }
topic = "${mode}/us/#"
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
