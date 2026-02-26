//! Chumsky-based parser for template strings
//!
//! Templates can contain:
//! - `${config.some.key}` - config variable references
//! - `${varname}` - template variables (for template rules)
//! - literal text
//!
//! This module uses a two-stage lexer/parser approach:
//! 1. Lexer: converts input string into a sequence of tokens with spans
//! 2. Parser: parses tokens into template components
//!
//! # Why a separate lexer from `condition.rs`?
//!
//! Templates and conditions are two distinct mini-languages with fundamentally
//! different requirements:
//!
//! - **Templates are whitespace-sensitive**: literal text must be preserved
//!   exactly as written, so this lexer produces coarse `Text` tokens containing
//!   arbitrary character sequences.
//!
//! - **Conditions are whitespace-insensitive**: expressions like
//!   `${connection.auth_method} == 'certificate'` need flexible whitespace
//!   around operators. That lexer uses `.padded()` to skip whitespace and
//!   produces fine-grained tokens (operators, strings, identifiers) that enable
//!   precise error messages like "expected `==`, found `=`".
//!
//! The coarse tokenisation this template lexer produces would be unhelpful for
//! condition parsing. The whole point of introducing separate lexing stages was
//! to improve error messages by breaking input into meaningful tokens, e.g.
//! grouping repeated `=` characters into a single operator. Using a lowest-
//! common-denominator tokenisation for both would defeat that purpose.

use std::fmt;
use std::marker::PhantomData;

use crate::config_toml::ConfigReference;
use crate::config_toml::ExpandError;

use super::OffsetSpan;
use chumsky::input::ValueInput;
use chumsky::input::WithContext;
use chumsky::prelude::*;
use chumsky::span::Span;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

/// Input type that produces OffsetSpan instead of SimpleSpan
type OffsetInput<'src> = WithContext<OffsetSpan, &'src str>;
/// Token with span
type Spanned<T> = (T, OffsetSpan);

// ============================================================================
// Lexer
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token<'src> {
    /// `${` - start of a variable reference
    VarStart,
    /// `.` - dot separator (inside variables)
    Dot,
    /// `}` - end of a variable reference
    VarEnd,
    /// An identifier in a variable reference like `config`, `c8y`, `url`
    Ident(&'src str),
    /// Literal text (outside variables)
    Text(&'src str),
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::VarStart => write!(f, "${{"),
            Token::Dot => write!(f, "."),
            Token::VarEnd => write!(f, "}}"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Text(s) => write!(f, "{s}"),
        }
    }
}

/// Lexer for template strings
///
/// Produces a flat token stream including both variable references and literal text.
/// Uses a recursive approach to handle the inside/outside variable context.
fn lexer<'src>() -> impl Parser<
    'src,
    OffsetInput<'src>,
    Vec<Spanned<Token<'src>>>,
    extra::Err<Rich<'src, char, OffsetSpan>>,
> {
    let var_start = just("${")
        .to(Token::VarStart)
        .map_with(|t, e| (t, e.span()));
    let var_end = just('}').to(Token::VarEnd).map_with(|t, e| (t, e.span()));
    let dot = just('.').to(Token::Dot).map_with(|t, e| (t, e.span()));

    // Identifier: alphanumeric + underscore
    let ident = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(Token::Ident)
        .map_with(|t, e| (t, e.span()))
        .labelled("identifier");

    // Inside a variable: dots, identifiers, and closing brace
    let var_inner = choice((dot, ident));

    // A complete variable: ${contents}
    // Returns a Vec of spanned tokens
    let variable = var_start
        .then(var_inner.repeated().collect::<Vec<_>>())
        .then(var_end.labelled("closing '}'"))
        .map(|((start, inner), end)| {
            let mut tokens = vec![start];
            tokens.extend(inner);
            tokens.push(end);
            tokens
        });

    // Text outside variables: anything that's not the start of a variable
    // We need to stop before `${` but allow standalone `$`
    let text = any()
        .and_is(just("${").not())
        .repeated()
        .at_least(1)
        .to_slice()
        .map(Token::Text)
        .map_with(|tok, e| vec![(tok, e.span())]);

    // Interleave variables and text, accumulating into a flat Vec
    choice((variable, text))
        .repeated()
        .collect::<Vec<_>>()
        .map(|vecs| vecs.into_iter().flatten().collect())
}

// ============================================================================
// Parser (operates on token stream)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateComponent<'src> {
    /// A config reference like `${config.c8y.url}` - stores just `c8y.url`
    Config(String, OffsetSpan),
    /// A mapper config reference like `${mapper.bridge.topic_prefix}` - stores just `bridge.topic_prefix`
    Mapper(String, OffsetSpan),
    /// The loop item variable `${item}`
    Item,
    /// Literal text
    Text(&'src str),
}

/// Parser for a dotted path like `c8y.mqtt_service.enabled`
fn dotted_path<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Vec<&'src str>, extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    let ident = select! { Token::Ident(s) => s }.labelled("identifier");

    ident
        .separated_by(just(Token::Dot))
        .at_least(1)
        .collect()
        .labelled("dotted path (e.g. 'c8y.url')")
}

/// Parser for variable contents - either `config.some.key`, `mapper.some.key`, or `item`
fn var_content_parser<'tokens, 'src: 'tokens, I>() -> impl Parser<
    'tokens,
    I,
    TemplateComponent<'src>,
    extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>,
> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    let config_ref = just(Token::Ident("config"))
        .ignore_then(just(Token::Dot))
        .ignore_then(
            dotted_path().map_with(|parts, e| TemplateComponent::Config(parts.join("."), e.span())),
        )
        .labelled("config reference (e.g. 'config.c8y.url')");

    let mapper_ref = just(Token::Ident("mapper"))
        .ignore_then(just(Token::Dot))
        .ignore_then(
            dotted_path().map_with(|parts, e| TemplateComponent::Mapper(parts.join("."), e.span())),
        )
        .labelled("mapper config reference (e.g. 'mapper.bridge.topic_prefix')");

    let item_var = just(Token::Ident("item"))
        .to(TemplateComponent::Item)
        .labelled("'item'");

    choice((config_ref, mapper_ref, item_var))
}

/// Parser for a complete variable: VarStart content VarEnd
fn variable_parser<'tokens, 'src: 'tokens, I>() -> impl Parser<
    'tokens,
    I,
    TemplateComponent<'src>,
    extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>,
> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    just(Token::VarStart)
        .ignore_then(var_content_parser())
        .then_ignore(just(Token::VarEnd))
}

/// Parser for a complete template (operates on token stream)
fn template_parser<'tokens, 'src: 'tokens, I>() -> impl Parser<
    'tokens,
    I,
    Vec<(TemplateComponent<'src>, OffsetSpan)>,
    extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>,
> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    let text = select! { Token::Text(s) => TemplateComponent::Text(s) };

    choice((variable_parser(), text))
        .map_with(|component, e| (component, e.span()))
        .repeated()
        .collect()
}

/// Parse a template string, returning components with their spans
pub fn parse_template(
    src: &toml::Spanned<String>,
) -> Result<Vec<(TemplateComponent<'_>, OffsetSpan)>, ExpandError> {
    // The input span includes the quotes in the original toml string, so bump by 1
    let offset = src.span().start + 1;
    let input = src.get_ref();

    // Lexer phase
    let (tokens, lex_errs) = lexer()
        .parse(input.as_str().with_context::<OffsetSpan>(offset))
        .into_output_errors();

    if let Some(e) = lex_errs.into_iter().next() {
        return Err(ExpandError {
            message: format!("Invalid character in variable: {e}"),
            help: None,
            span: e.span().into_range(),
        });
    }

    let tokens = tokens.expect("tokens should exist if no errors");

    // Parser phase
    let len = input.len();
    let eoi = offset + len;

    let (components, parse_errs) = template_parser()
        .parse(
            tokens
                .as_slice()
                .map(OffsetSpan::new(0, eoi..eoi), |(t, s)| (t, s)),
        )
        .into_output_errors();

    if let Some(e) = parse_errs.into_iter().next() {
        return Err(ExpandError {
            message: e.to_string(),
            help: None,
            span: e.span().into_range(),
        });
    }

    Ok(components.expect("components should exist if no errors"))
}

/// Parser for a config reference: `${config.key}` - returns (key_string, key_span)
fn config_ref_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, (String, OffsetSpan), extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>>
       + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    just(Token::VarStart)
        .ignore_then(just(Token::Ident("config")))
        .ignore_then(just(Token::Dot))
        .ignore_then(dotted_path().map_with(|parts, e| (parts.join("."), e.span())))
        .then_ignore(just(Token::VarEnd))
        .labelled("config reference (e.g. '${config.c8y.url}')")
}

/// Parse a config reference like `${config.c8y.url}`
pub fn parse_config_reference<T>(
    src: &toml::Spanned<String>,
) -> Result<ConfigReference<T>, ExpandError> {
    // The input span includes the quotes in the original toml string, so bump by 1
    let offset = src.span().start + 1;
    let input = src.get_ref();

    // Lexer phase
    let (tokens, lex_errs) = lexer()
        .parse(input.as_str().with_context::<OffsetSpan>(offset))
        .into_output_errors();

    if let Some(e) = lex_errs.into_iter().next() {
        // Improve error message for unclosed braces
        let message = if e.to_string().contains("closing '}'") {
            "config reference must end with }".to_string()
        } else {
            format!("Invalid character in config reference: {e}")
        };
        return Err(ExpandError {
            message,
            help: None,
            span: e.span().into_range(),
        });
    }

    let tokens = tokens.expect("tokens should exist if no errors");

    // Parser phase
    let len = input.len();
    let eoi = offset + len;

    let (result, parse_errs) = config_ref_parser()
        .then_ignore(end())
        .parse(
            tokens
                .as_slice()
                .map(OffsetSpan::new(0, eoi..eoi), |(t, s)| (t, s)),
        )
        .into_output_errors();

    if let Some(e) = parse_errs.into_iter().next() {
        return Err(ExpandError {
            message: "expected config reference".into(),
            help: Some("Use format: ${config.key}".into()),
            span: e.span().into_range(),
        });
    }

    let (key, key_span) = result.expect("result should exist if no errors");
    Ok(ConfigReference(
        toml::Spanned::new(key_span.into_range(), key),
        PhantomData,
    ))
}

/// Resolves a dotted path (e.g. `bridge.topic_prefix`) against a TOML table.
///
/// Returns the string representation of the value, or an error if the key is not found
/// or the value is not a scalar.
fn expand_mapper_key(
    path: &str,
    table: &toml::Table,
    span: std::ops::Range<usize>,
) -> Result<String, ExpandError> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current_table = table;

    // Navigate through all intermediate parts
    for &part in &parts[..parts.len() - 1] {
        match current_table.get(part) {
            Some(toml::Value::Table(t)) => current_table = t,
            Some(_) => {
                return Err(ExpandError {
                    message: format!("Cannot navigate path '{path}': '{part}' is not a table"),
                    help: None,
                    span,
                });
            }
            None => {
                return Err(ExpandError {
                    message: format!("Key '{path}' not found in mapper config"),
                    help: Some(format!(
                        "Check that '{part}' exists in your mapper's tedge.toml"
                    )),
                    span,
                });
            }
        }
    }

    let last_part = parts.last().expect("path must have at least one part");
    match current_table.get(*last_part) {
        Some(toml::Value::String(s)) => Ok(s.clone()),
        Some(toml::Value::Integer(i)) => Ok(i.to_string()),
        Some(toml::Value::Float(f)) => Ok(f.to_string()),
        Some(toml::Value::Boolean(b)) => Ok(b.to_string()),
        Some(_) => Err(ExpandError {
            message: format!("Mapper config key '{path}' is not a scalar value"),
            help: Some(
                "Only string, integer, float, and boolean values can be used in templates".into(),
            ),
            span,
        }),
        None => Err(ExpandError {
            message: format!("Key '{path}' not found in mapper config"),
            help: Some(format!(
                "Check that '{last_part}' exists in your mapper's tedge.toml"
            )),
            span,
        }),
    }
}

/// Expand a template that only contains config references (no template variables)
pub fn expand_config_template(
    src: &toml::Spanned<String>,
    config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
    mapper_config: Option<&toml::Table>,
) -> Result<String, ExpandError> {
    let components = parse_template(src)?;
    let mut result = String::new();

    for (component, span) in components {
        match component {
            TemplateComponent::Text(text) => result.push_str(text),
            TemplateComponent::Config(key, span) => {
                let value =
                    super::super::expand_config_key(&key, config, cloud_profile, span.into())?;
                result.push_str(&value);
            }
            TemplateComponent::Mapper(path, key_span) => match mapper_config {
                Some(table) => {
                    let value = expand_mapper_key(&path, table, key_span.into())?;
                    result.push_str(&value);
                }
                None => {
                    return Err(ExpandError {
                        message: format!(
                            "Variable '${{mapper.{path}}}' is only valid in custom mapper bridge rules"
                        ),
                        help: Some(
                            "Create a tedge.toml in your mapper directory to use ${mapper.*}"
                                .into(),
                        ),
                        span: span.into(),
                    });
                }
            },
            TemplateComponent::Item => {
                return Err(ExpandError {
                    message: "Variable 'item' is only valid inside template rules".into(),
                    help: Some("Use 'config.<key>' for config references".into()),
                    span: span.into(),
                });
            }
        }
    }

    Ok(result)
}

/// Context for expanding template rule topics
pub struct TemplateContext<'a> {
    pub tedge: &'a TEdgeConfig,
    pub loop_var_value: &'a str,
    pub mapper_config: Option<&'a toml::Table>,
}

/// Expand a template that can contain both config references and the loop item variable
pub fn expand_loop_template(
    src: &toml::Spanned<String>,
    ctx: &TemplateContext<'_>,
    cloud_profile: Option<&ProfileName>,
) -> Result<String, ExpandError> {
    let components = parse_template(src)?;
    let mut result = String::new();

    for (component, span) in components {
        match component {
            TemplateComponent::Text(text) => result.push_str(text),
            TemplateComponent::Config(key, key_span) => {
                let value = super::super::expand_config_key(
                    &key,
                    ctx.tedge,
                    cloud_profile,
                    key_span.into(),
                )?;
                result.push_str(&value);
            }
            TemplateComponent::Mapper(path, key_span) => match ctx.mapper_config {
                Some(table) => {
                    let value = expand_mapper_key(&path, table, key_span.into())?;
                    result.push_str(&value);
                }
                None => {
                    return Err(ExpandError {
                        message: format!(
                            "Variable '${{mapper.{path}}}' is only valid in custom mapper bridge rules"
                        ),
                        help: Some(
                            "Create a tedge.toml in your mapper directory to use ${mapper.*}"
                                .into(),
                        ),
                        span: span.into(),
                    });
                }
            },
            TemplateComponent::Item => {
                result.push_str(ctx.loop_var_value);
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::super::super::test_helpers::*;
    use super::*;

    #[test]
    fn parses_plain_text() {
        let input = toml_spanned("hello world");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Text("hello world"));
    }

    #[test]
    fn parses_config_reference() {
        let input = toml_spanned("${config.c8y.url}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);
        let TemplateComponent::Config(ref var_name, span) = components[0].0 else {
            panic!("Expected config reference, got: {:?}", components[0].0);
        };
        assert_eq!(var_name, "c8y.url");
        assert_eq!(extract_toml_span(&input, span.into_range()), *var_name);
    }

    #[test]
    fn parses_item_variable() {
        let input = toml_spanned("${item}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Item);
    }

    #[test]
    fn parses_mixed_template() {
        let input = toml_spanned("prefix/${config.c8y.bridge.topic_prefix}/${item}/suffix");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 5);
        assert_eq!(components[0].0, TemplateComponent::Text("prefix/"));
        let TemplateComponent::Config(ref var_name, span) = components[1].0 else {
            panic!("Expected config reference, got: {:?}", components[1].0);
        };
        assert_eq!(var_name, "c8y.bridge.topic_prefix");
        assert_eq!(extract_toml_span(&input, span.into_range()), *var_name);
        assert_eq!(components[2].0, TemplateComponent::Text("/"));
        assert_eq!(components[3].0, TemplateComponent::Item);
        assert_eq!(components[4].0, TemplateComponent::Text("/suffix"));
    }

    #[test]
    fn template_tokens_can_be_stringified() {
        let raw_input = "prefix/${config.c8y.bridge.topic_prefix}/${item}/suffix";
        let components = lexer()
            .parse(raw_input.with_context(0))
            .into_result()
            .unwrap();
        let stringified = components
            .into_iter()
            .map(|(c, _span)| c.to_string())
            .collect::<String>();
        assert_eq!(stringified, raw_input);
    }

    #[test]
    fn config_template_rejects_item_variable() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${item}");
        let result = expand_config_template(&input, &config, None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("only valid inside template rules"),
            "{}",
            err.message
        );
        assert_eq!(extract_toml_span(&input, err.span), "${item}");
    }

    #[test]
    fn rejects_unknown_variables_at_parse_time() {
        let input = toml_spanned("${unknown}");
        let result = parse_template(&input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error should mention what's valid
        assert!(
            err.message.contains("config") || err.message.contains("item"),
            "Error should mention valid options: {}",
            err.message
        );
    }

    #[test]
    fn config_template_rejects_non_alphanumeric_characters() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${test@me}");
        let result = expand_config_template(&input, &config, None, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Invalid character"), "{}", err.message);
        assert_eq!(extract_toml_span(&input, err.span), "@");
    }

    #[test]
    fn template_template_expands_loop_variable() {
        let config = TEdgeConfig::load_toml_str("");
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_value: "my-topic",
            mapper_config: None,
        };
        let result = expand_loop_template(&toml_spanned("s/uc/${item}"), &ctx, None).unwrap();
        assert_eq!(result, "s/uc/my-topic");
    }

    #[test]
    fn loop_template_rejects_unknown_variables_at_parse_time() {
        let config = TEdgeConfig::load_toml_str("");
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_value: "my-topic",
            mapper_config: None,
        };
        let result = expand_loop_template(&toml_spanned("${unknown}"), &ctx, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Error comes from parse time, should mention valid options
        assert!(
            err.message.contains("config") || err.message.contains("item"),
            "Error should mention valid options: {}",
            err.message
        );
    }

    // ========================================================================
    // ${mapper.*} tests
    // ========================================================================

    #[test]
    fn parses_mapper_reference() {
        let input = toml_spanned("${mapper.bridge.topic_prefix}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);
        let TemplateComponent::Mapper(ref path, span) = components[0].0 else {
            panic!("Expected mapper reference, got: {:?}", components[0].0);
        };
        assert_eq!(path, "bridge.topic_prefix");
        assert_eq!(extract_toml_span(&input, span.into_range()), *path);
    }

    #[test]
    fn config_template_expands_simple_mapper_key() {
        let config = TEdgeConfig::load_toml_str("");
        let mapper: toml::Table = toml::toml! { topic_prefix = "tb" };
        let input = toml_spanned("${mapper.topic_prefix}");
        let result = expand_config_template(&input, &config, None, Some(&mapper)).unwrap();
        assert_eq!(result, "tb");
    }

    #[test]
    fn config_template_expands_nested_mapper_key() {
        let config = TEdgeConfig::load_toml_str("");
        let mapper: toml::Table = toml::toml! {
            [bridge]
            topic_prefix = "tb"
        };
        let input = toml_spanned("${mapper.bridge.topic_prefix}");
        let result = expand_config_template(&input, &config, None, Some(&mapper)).unwrap();
        assert_eq!(result, "tb");
    }

    #[test]
    fn config_template_errors_on_missing_mapper_key() {
        let config = TEdgeConfig::load_toml_str("");
        let mapper: toml::Table = toml::Table::new();
        let input = toml_spanned("${mapper.nonexistent.key}");
        let err = expand_config_template(&input, &config, None, Some(&mapper)).unwrap_err();
        assert!(
            err.message.contains("nonexistent.key") || err.message.contains("nonexistent"),
            "{}",
            err.message
        );
    }

    #[test]
    fn config_template_errors_when_no_mapper_config() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${mapper.bridge.topic_prefix}");
        let err = expand_config_template(&input, &config, None, None).unwrap_err();
        assert!(
            err.message.contains("mapper"),
            "Error should mention mapper: {}",
            err.message
        );
    }

    #[test]
    fn loop_template_expands_mapper_key() {
        let config = TEdgeConfig::load_toml_str("");
        let mapper: toml::Table = toml::toml! { topic_prefix = "tb" };
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_value: "telemetry",
            mapper_config: Some(&mapper),
        };
        let result =
            expand_loop_template(&toml_spanned("${mapper.topic_prefix}/${item}"), &ctx, None)
                .unwrap();
        assert_eq!(result, "tb/telemetry");
    }

    #[test]
    fn config_template_combines_mapper_and_config_references() {
        let config = TEdgeConfig::load_toml_str("mqtt.port = 1883");
        let mapper: toml::Table = toml::toml! { topic_prefix = "tb" };
        let input = toml_spanned("${mapper.topic_prefix}/${config.mqtt.port}");
        let result = expand_config_template(&input, &config, None, Some(&mapper)).unwrap();
        assert_eq!(result, "tb/1883");
    }

    // ========================================================================
    // Span/offset tests
    // ========================================================================

    #[test]
    fn span_points_to_config_reference() {
        let input = toml_spanned("${config.c8y.url}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);

        let (_, span) = &components[0];
        assert_eq!(
            extract_toml_span(&input, span.into_range()),
            "${config.c8y.url}"
        );
    }

    #[test]
    fn span_points_to_item_variable() {
        let input = toml_spanned("${item}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);

        let (_, span) = &components[0];
        assert_eq!(extract_toml_span(&input, span.into_range()), "${item}");
    }

    #[test]
    fn spans_in_mixed_template_point_to_correct_components() {
        let input = toml_spanned("prefix/${config.key}/${item}/suffix");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 5);

        // "prefix/"
        assert_eq!(
            extract_toml_span(&input, components[0].1.into_range()),
            "prefix/"
        );
        // "${config.key}"
        assert_eq!(
            extract_toml_span(&input, components[1].1.into_range()),
            "${config.key}"
        );
        // "/"
        assert_eq!(extract_toml_span(&input, components[2].1.into_range()), "/");
        // "${item}"
        assert_eq!(
            extract_toml_span(&input, components[3].1.into_range()),
            "${item}"
        );
        // "/suffix"
        assert_eq!(
            extract_toml_span(&input, components[4].1.into_range()),
            "/suffix"
        );
    }

    #[test]
    fn error_offset_for_unknown_variable_at_start() {
        // Unknown variables are now rejected at parse time
        let input = toml_spanned("${unknown}/rest");
        let err = parse_template(&input).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span.clone()), "unknown");
    }

    #[test]
    fn error_offset_for_unknown_variable_in_middle() {
        // Unknown variables are now rejected at parse time
        let input = toml_spanned("prefix/${unknown}/suffix");
        let err = parse_template(&input).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span.clone()), "unknown");
    }

    #[test]
    fn error_offset_for_invalid_config_key() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("prefix/${config.invalid.key}/suffix");
        let err = expand_config_template(&input, &config, None, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "invalid.key");
    }

    #[test]
    fn error_offset_for_unknown_loop_variable() {
        // Unknown variables are now rejected at parse time
        let input = toml_spanned("start/${wrong}/end");
        let err = parse_template(&input).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span.clone()), "wrong");
    }

    #[test]
    fn error_offset_with_multiple_variables_points_to_failing_one() {
        // First variable is valid, second is invalid - rejected at parse time
        let input = toml_spanned("${item}/${unknown}");
        let err = parse_template(&input).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span.clone()), "unknown");
    }
}
