mod parsing;
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
use yansi::Paint as _;

use parsing::parse_condition_with_error;
use parsing::template::expand_config_template;
use parsing::template::expand_loop_template;
use parsing::template::TemplateContext;

use crate::config_toml::parsing::template::parse_config_reference;

#[cfg(test)]
mod test_helpers;

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

#[derive(Debug)]
pub enum NonExpansionReason {
    ConditionIsFalse {
        /// The span to highlight (points to the most relevant part, e.g., config reference)
        span: std::ops::Range<usize>,
        message: String,
        /// The span of the entire rule that was skipped
        rule_span: Option<std::ops::Range<usize>>,
    },
    LoopSourceEmpty {
        src: Spanned<Iterable>,
        message: String,
        /// The span of the entire rule that was skipped
        rule_span: std::ops::Range<usize>,
    },
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
        mapper_config: Option<&toml::Table>,
    ) -> Result<(Vec<ExpandedBridgeRule>, Vec<NonExpansionReason>), Vec<ExpandError>> {
        let static_cfg = || StaticTemplateConfig {
            tedge: config,
            mapper_config,
        };
        let expand_prefix = |prefix: &Option<_>, name| match prefix.as_ref() {
            Some(prefix) => expand_spanned(
                prefix,
                static_cfg(),
                cloud_profile,
                &format!("Failed to expand {name}"),
            )
            .map(PrefixExpansionState::Expanded),
            None => Ok(PrefixExpansionState::NotDefined),
        };
        let mut errors = Vec::new();
        let mut non_expansion_reasons = Vec::new();
        // Returns (result, parsed_condition) so we can use the parsed condition for explanations
        let expand_condition =
            |condition: Option<&Spanned<String>>,
             context: &str|
             -> Result<Option<(bool, Spanned<Condition>)>, Vec<ExpandError>> {
                condition
                    .map(|s| {
                        let parsed: Spanned<Condition> = parse_condition_with_error(s)?;
                        let result = expand_spanned(
                            &parsed,
                            (config, auth_method),
                            cloud_profile,
                            &format!("Failed to expand {context} condition"),
                        )
                        .map_err(|err| vec![err])?;
                        Ok((result, parsed))
                    })
                    .transpose()
            };

        let file_condition =
            expand_condition(self.r#if.as_ref(), "global").unwrap_or_else(|mut e| {
                errors.append(&mut e);
                None
            });
        let template_disabled = matches!(&file_condition, Some((false, _)));
        if let Some((false, ref parsed)) = file_condition {
            let (message, span) =
                explain_false_condition(parsed, config, auth_method, cloud_profile);
            non_expansion_reasons.push(NonExpansionReason::ConditionIsFalse {
                span,
                message,
                rule_span: None, // File-level condition has no containing rule
            });
        }

        let local_prefix = expand_prefix(&self.local_prefix, "local_prefix").unwrap_or_else(|e| {
            errors.push(e);
            PrefixExpansionState::Error
        });
        let remote_prefix =
            expand_prefix(&self.remote_prefix, "remote_prefix").unwrap_or_else(|e| {
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
            let rule_disabled = template_disabled || matches!(&rule_condition, Some((false, _)));
            if let Some((false, ref parsed)) = rule_condition {
                let (message, span) =
                    explain_false_condition(parsed, config, auth_method, cloud_profile);
                non_expansion_reasons.push(NonExpansionReason::ConditionIsFalse {
                    span,
                    message,
                    rule_span: Some(rule_span.clone()),
                });
            }

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
                topic: expand_spanned(
                    &rule.topic,
                    static_cfg(),
                    cloud_profile,
                    "Failed to expand topic",
                )
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
            let template_rule_disabled =
                template_disabled || matches!(&template_condition, Some((false, _)));
            if let Some((false, ref parsed)) = template_condition {
                let (message, span) =
                    explain_false_condition(parsed, config, auth_method, cloud_profile);
                non_expansion_reasons.push(NonExpansionReason::ConditionIsFalse {
                    span,
                    message,
                    rule_span: Some(template_span.clone()),
                });
            }

            let iterable = template
                .r#for
                .expand(config, cloud_profile)
                .unwrap_or_else(|mut e| {
                    e.message = format!("Failed to expand 'for' reference: {}", e.message);
                    errors.push(e);
                    <_>::default()
                });

            let template_local_prefix = expand_prefix(&template.local_prefix, "local_prefix")
                .unwrap_or_else(|e| {
                    errors.push(e);
                    PrefixExpansionState::Error
                });
            let template_remote_prefix = expand_prefix(&template.remote_prefix, "remote_prefix")
                .unwrap_or_else(|e| {
                    errors.push(e);
                    PrefixExpansionState::Error
                });

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
                template_span.clone(),
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
                    tedge: config,
                    mapper_config,
                };
                non_expansion_reasons.push(NonExpansionReason::LoopSourceEmpty {
                    src: template.r#for.clone(),
                    message: "iterator is empty".into(),
                    rule_span: template_span.clone(),
                });
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
                    tedge: config,
                    mapper_config,
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
            Ok((expanded_rules, non_expansion_reasons))
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
    r#for: Spanned<Iterable>,
    topic: Spanned<LoopTemplate>,
    local_prefix: Option<Spanned<Template>>,
    remote_prefix: Option<Spanned<Template>>,
    direction: Direction,
    r#if: Option<Spanned<String>>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Inbound,
    Outbound,
    Bidirectional,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Condition {
    AuthMethod(AuthMethod),
    Is(bool, ConfigReference<bool>),
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

impl Expandable for Condition {
    type Target = bool;
    type Config<'a> = (&'a TEdgeConfig, AuthMethod);

    fn expand(
        &self,
        config: Self::Config<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, ExpandError> {
        match self {
            Self::AuthMethod(auth_method) => Ok(*auth_method == config.1),
            Self::Is(true, config_ref) => config_ref.expand(config.0, cloud_profile),
            Self::Is(false, config_ref) => Ok(!config_ref.expand(config.0, cloud_profile)?),
        }
    }
}

/// Generates a human-readable explanation of why a condition evaluated to false,
/// along with the most relevant span to highlight.
fn explain_false_condition(
    condition: &Spanned<Condition>,
    config: &TEdgeConfig,
    auth_method: AuthMethod,
    cloud_profile: Option<&ProfileName>,
) -> (String, std::ops::Range<usize>) {
    match condition.get_ref() {
        Condition::AuthMethod(expected) => (
            format!(
                "auth method is {}, not {}",
                auth_method.yellow(),
                expected.green()
            ),
            // For auth method, highlight the whole condition
            condition.span(),
        ),
        Condition::Is(expected, config_ref) => {
            let message = match config_ref.expand(config, cloud_profile) {
                Ok(_) => format!("{} is {}", config_ref.0.cyan(), (!*expected).yellow()),
                Err(_) => format!("{} could not be read", config_ref.0.cyan()),
            };
            // Highlight the config reference specifically
            (message, config_ref.span())
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct Template(Spanned<String>);

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
struct LoopTemplate(Spanned<String>);

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(untagged)]
pub enum Iterable {
    Config(String),
    Literal(Vec<String>),
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(into = "String")]
pub(crate) struct ConfigReference<Target>(Spanned<String>, PhantomData<Target>);

impl<Target> Clone for ConfigReference<Target> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), self.1)
    }
}

impl<Target> ConfigReference<Target> {
    pub fn span(&self) -> std::ops::Range<usize> {
        self.0.span()
    }
}

#[cfg(test)]
impl<Target> FromStr for ConfigReference<Target> {
    type Err = ExpandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ser = String::new();
        serde::Serialize::serialize(s, toml::ser::ValueSerializer::new(&mut ser)).unwrap();
        parse_config_reference(
            &Spanned::<String>::deserialize(toml::de::ValueDeserializer::parse(&ser).unwrap())
                .unwrap(),
        )
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
    ) -> Result<Self::Target, ExpandError>;
}

/// Helper to expand a spanned template and convert errors to ExpandError
fn expand_spanned<T: Expandable>(
    spanned: &Spanned<T>,
    config: T::Config<'_>,
    cloud_profile: Option<&ProfileName>,
    context: &str,
) -> Result<T::Target, ExpandError> {
    spanned
        .get_ref()
        .expand(config, cloud_profile)
        .map_err(|e| ExpandError {
            message: format!("{context}: {}", e.message),
            help: e.help,
            span: e.span,
        })
}

impl Expandable for Spanned<Iterable> {
    type Target = TemplatesSet;
    type Config<'a>
        = &'a TEdgeConfig
    where
        Self: 'a;

    fn expand(
        &self,
        tedge_config: &TEdgeConfig,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, ExpandError> {
        match self.get_ref() {
            Iterable::Config(config_ref) => {
                parse_config_reference(&toml::Spanned::new(self.span(), config_ref.clone()))?
                    .expand(tedge_config, cloud_profile)
            }
            Iterable::Literal(values) => Ok(TemplatesSet(values.clone())),
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
    ) -> Result<Self::Target, ExpandError> {
        let key: ReadableKey =
            self.0
                .get_ref()
                .parse()
                .map_err(|e: ParseKeyError| ExpandError {
                    message: e.to_string(),
                    help: None,
                    span: self.0.span(),
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
            ExpandError {
                message,
                help,
                span: self.0.span(),
            }
        })?;

        // Try to deserialize as TOML first (for complex types like TemplatesSet)
        // If that fails, fall back to FromStr parsing (for simple string types)
        let deser = toml::de::ValueDeserializer::parse(&value);
        deser
            .and_then(Target::deserialize)
            .or_else(|_| value.parse())
            .map_err(|e: Target::Err| ExpandError {
                message: e.to_string(),
                help: None,
                span: self.0.span(),
            })
    }
}

struct TemplateConfig<'a> {
    tedge: &'a TEdgeConfig,
    r#for: &'a str,
    mapper_config: Option<&'a toml::Table>,
}

struct StaticTemplateConfig<'a> {
    tedge: &'a TEdgeConfig,
    mapper_config: Option<&'a toml::Table>,
}

/// Expands a tedge.toml config key and return its value
///
/// Expects var_name to start with "config." (e.g., "config.c8y.url"),
/// strips it, and reads the config value
pub(crate) fn expand_config_key(
    var_name: &str,
    config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
    span: std::ops::Range<usize>,
) -> Result<String, ExpandError> {
    let key: ReadableKey = var_name.parse().map_err({
        let span = span.clone();
        |e: ParseKeyError| ExpandError {
            message: e.to_string(),
            help: None,
            span,
        }
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
        ExpandError {
            message,
            help,
            span,
        }
    })
}

impl Expandable for Template {
    type Target = String;
    type Config<'a>
        = StaticTemplateConfig<'a>
    where
        Self: 'a;

    fn expand(
        &self,
        config: StaticTemplateConfig<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, ExpandError> {
        expand_config_template(&self.0, config.tedge, cloud_profile, config.mapper_config)
    }
}

impl Expandable for LoopTemplate {
    type Target = String;
    type Config<'a>
        = TemplateConfig<'a>
    where
        Self: 'a;

    fn expand(
        &self,
        config: TemplateConfig<'_>,
        cloud_profile: Option<&ProfileName>,
    ) -> Result<Self::Target, ExpandError> {
        let ctx = TemplateContext {
            tedge: config.tedge,
            loop_var_value: config.r#for,
            mapper_config: config.mapper_config,
        };
        expand_loop_template(&self.0, &ctx, cloud_profile)
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
                for = "${config.c8y.topics}"
                topic = "${item}"
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
                for = "${config.c8y.topics}"
                topic = "${item}"
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
        fn config_reference_rejects_missing_opening_dollar() {
            let reference = "c8y.topics.e}";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            let err = result.unwrap_err();
            assert!(
                err.message.contains("expected config reference"),
                "{}",
                err.message
            );
        }

        #[test]
        fn config_reference_rejects_missing_config_prefix() {
            let reference = "${c8y.topics.e}";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            let err = result.unwrap_err();
            assert!(
                err.message.contains("expected config reference"),
                "{}",
                err.message
            );
        }

        #[test]
        fn config_reference_rejects_missing_suffix() {
            let reference = "${config.c8y.mqtt_service.topics";
            let result: Result<ConfigReference<TemplatesSet>, _> = reference.parse();
            assert!(result.is_err());
            assert!(result.unwrap_err().message.contains("must end with }"));
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
        fn expands_config_reference() {
            let reference: ConfigReference<_> =
                "${config.c8y.mqtt_service.enabled}".parse().unwrap();
            let condition = Condition::Is(true, reference);
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = true");
            assert!(condition
                .expand((&tedge_config, AuthMethod::Certificate), None)
                .unwrap());
        }

        #[test]
        fn expands_negated_config_reference() {
            let reference: ConfigReference<_> =
                "${config.c8y.mqtt_service.enabled}".parse().unwrap();
            let condition = Condition::Is(false, reference);
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = true");
            assert!(!condition
                .expand((&tedge_config, AuthMethod::Certificate), None)
                .unwrap());
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
for = "${config.c8y.topics.e}"
topic = "te/device/main///e/${suffix}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let span = config.template_rules[0].get_ref().r#for.span();

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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
for = ['a', 'b']
if = "${config.c8y.mqtt_service.enabled}"
topic = "${item}/${config.invalid.key}/suffix"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.mqtt_service.enabled = false");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(
                errs.len(),
                1,
                "There should be precisely 1 error, got: {errs:?}"
            );
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
for = "${config.c8y.smartrest.templates}"
                
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
for = "${config.c8y.smartrest.templates}"
                
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
                    .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
for = "${config.c8y.smartrest.templates}"
topic = "s/dc/${item}"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.smartrest.templates = ["a", "b"]
            "#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(expanded.0.len(), 2);
            assert_eq!(expanded.0[0].topic, "s/dc/a");
            assert_eq!(expanded.0[1].topic, "s/dc/b");
        }

        #[test]
        fn template_rules_can_expand_literal_arrays() {
            let toml = r#"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = ['s', 't', 'q', 'c']
topic = "${item}/us/#"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str(
                r#"
c8y.smartrest.templates = ["a", "b"]
            "#,
            );

            let expanded = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(expanded.0.len(), 4);
            assert_eq!(expanded.0[0].topic, "s/us/#");
            assert_eq!(expanded.0[1].topic, "t/us/#");
            assert_eq!(expanded.0[2].topic, "q/us/#");
            assert_eq!(expanded.0[3].topic, "c/us/#");
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
for = "${config.c8y.smartrest.templates}"
topic = "${config.something.unknown}/${item}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config =
                tedge_config::TEdgeConfig::load_toml_str("c8y.smartrest.templates = []");

            // Save the span before we consume config
            let topic_span = config.template_rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
for = "${config.c8y.smartrest.templates}"
topic = "${config.second}/${item}"
direction = "outbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            // Save the span before we consume config
            let topic_span = config.rules[0].get_ref().topic.span();
            let template_topic_span = config.template_rules[0].get_ref().topic.span();

            // This should fail on the first variable
            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
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
for = "${config.c8y.smartrest.templates}"
topic = "s/uc/${item}"
direction = "outbound"

[[template_rule]]
for = "${config.c8y.smartrest.templates}"
topic = "s/dc/${item}"
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            // Should create 6 rules: 3 outbound + 3 inbound
            assert_eq!(expanded.0.len(), 6);

            // Check outbound rules (s/uc/${@for})
            assert_eq!(expanded.0[0].topic, "s/uc/template1");
            assert_eq!(expanded.0[0].local_prefix, "c8y/");
            assert_eq!(expanded.0[0].remote_prefix, "");
            assert!(matches!(expanded.0[0].direction, Direction::Outbound));

            assert_eq!(expanded.0[1].topic, "s/uc/template2");
            assert_eq!(expanded.0[2].topic, "s/uc/template3");

            // Check inbound rules (s/dc/${@for})
            assert_eq!(expanded.0[3].topic, "s/dc/template1");
            assert_eq!(expanded.0[3].local_prefix, "c8y/");
            assert_eq!(expanded.0[3].remote_prefix, "");
            assert!(matches!(expanded.0[3].direction, Direction::Inbound));

            assert_eq!(expanded.0[4].topic, "s/dc/template2");
            assert_eq!(expanded.0[5].topic, "s/dc/template3");
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
                    None,
                )
                .unwrap();

            assert_eq!(expanded.0.len(), 1);

            assert_eq!(expanded.0[0].topic, "s/us");
            assert_eq!(expanded.0[0].local_prefix, "test/");
            assert_eq!(expanded.0[0].remote_prefix, "");
            assert!(matches!(expanded.0[0].direction, Direction::Outbound));
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(expanded.0.len(), 0);
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(expanded.0.len(), 1);
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
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(with_certificate.0.len(), 0);

            let with_password = config
                .expand(&tedge_config, AuthMethod::Password, None, None)
                .unwrap();

            assert_eq!(with_password.0.len(), 1);
        }

        #[test]
        fn template_rules_can_be_conditionally_enabled() {
            let toml = r##"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
if = "${connection.auth_method} == 'certificate'"
for = ['s', 't', 'q', 'c']
topic = "${item}/us/#"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let with_certificate = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap();

            assert_eq!(with_certificate.0.len(), 4);

            let with_password = config
                .expand(&tedge_config, AuthMethod::Password, None, None)
                .unwrap();

            assert_eq!(with_password.0.len(), 0);
        }

        #[test]
        fn topic_does_not_error_when_for_does() {
            let toml = r##"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = "${config.c8y.unknown.key}"
topic = "s/uc/${item}"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errors = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(
                errors.len(),
                1,
                "Expected only 1 error, actual errors were {errors:?}"
            );

            assert_eq!(&toml[errors[0].span.clone()], "c8y.unknown.key");
        }

        #[test]
        fn unknown_keys_have_correct_span_info() {
            let toml = r##"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
if = "${config.not.a.real.key}"
topic = "a/b"
direction = "outbound"
"##;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errors = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(
                errors.len(),
                1,
                "Expected only 1 error, actual errors were {errors:?}"
            );

            assert_eq!(&toml[errors[0].span.clone()], "not.a.real.key");
        }

        #[test]
        fn config_reference_serializes_to_original_value() {
            let original: ConfigReference<String> = "${config.c8y.url}".parse().unwrap();
            let mut value = String::new();
            original
                .serialize(toml::ser::ValueSerializer::new(&mut value))
                .unwrap();
            let deserialized: Spanned<String> =
                <_>::deserialize(toml::de::ValueDeserializer::parse(&value).unwrap()).unwrap();
            assert_eq!(parse_config_reference(&deserialized).unwrap(), original);
        }

        #[test]
        fn missing_config_prefix_in_local_prefix_gives_useful_error() {
            // User wrote ${c8y.bridge.topic_prefix} instead of ${config.c8y.bridge.topic_prefix}
            let toml = r#"
local_prefix = "${c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[rule]]
topic = "foo"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1, "Expected 1 error, got: {errs:?}");
            let err = &errs[0];

            // Help should suggest using config. prefix
            assert!(
                err.message.contains("config."),
                "Message should suggest using config. prefix, got: {:?}",
                err.message
            );

            // Span should point to the problematic variable reference in the toml
            let error_text = &toml[err.span.clone()];
            assert_eq!(
                error_text, "c8y",
                "Span should point to the variable reference"
            );
        }

        #[test]
        fn unfinished_config_reference_gives_useful_error() {
            let toml = r#"
local_prefix = "${config.c8y.bridge.topic_prefix}/"
remote_prefix = ""

[[template_rule]]
for = "${config.c8y.smartrest.templates"
topic = "${item}"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1, "Expected 1 error, got: {errs:?}");
            let err = &errs[0];

            assert!(
                err.message.contains("config reference must end with }"),
                "Message should be an end of input error, got: {:?}",
                err.message
            );

            let start = err.span.start;
            let end = err.span.end;

            // Span should point to the problematic variable reference in the toml
            let error_text = &toml[start..end + 1];
            assert_eq!(
                error_text, "\"",
                "Span should point to the end of the string"
            );
        }

        #[test]
        fn unfinished_template_variable_gives_useful_error() {
            let toml = r#"
local_prefix = "${config.c8y.bridge.topic_prefix"
remote_prefix = ""

[[rule]]
topic = "test"
direction = "inbound"
"#;
            let config: PersistedBridgeConfig = toml::from_str(toml).unwrap();
            let tedge_config = tedge_config::TEdgeConfig::load_toml_str("");

            let errs = config
                .expand(&tedge_config, AuthMethod::Certificate, None, None)
                .unwrap_err();

            assert_eq!(errs.len(), 1, "Expected 1 error, got: {errs:?}");
            let err = &errs[0];

            assert!(
                err.message.contains("found end of input"),
                "Message should be an end of input error, got: {:?}",
                err.message
            );

            let start = err.span.start;
            let end = err.span.end;

            // Span should point to the problematic variable reference in the toml
            let error_text = &toml[start..end + 1];
            assert_eq!(
                error_text, "\"",
                "Span should point to the end of the string"
            );
        }
    }
}
