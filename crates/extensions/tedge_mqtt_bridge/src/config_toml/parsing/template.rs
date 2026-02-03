//! Chumsky-based parser for template strings
//!
//! Templates can contain:
//! - `${config.some.key}` - config variable references
//! - `${varname}` - template variables (for template rules)
//! - literal text
//!
//! This module uses a two-stage approach for variable parsing:
//! 1. Outer parser: finds `${...}` blocks and literal text
//! 2. Inner lexer/parser: tokenizes and parses the contents inside `${...}`

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

// ============================================================================
// Lexer for variable contents (inside `${...}`)
// ============================================================================

/// Token with span for the inner variable parser
type InnerSpan<T> = (T, OffsetSpan);

#[derive(Clone, Debug, PartialEq, Eq)]
enum Token<'src> {
    /// `.` - dot separator
    Dot,
    /// An identifier like `config`, `c8y`, `url`
    Ident(&'src str),
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::Dot => write!(f, "."),
            Token::Ident(s) => write!(f, "{s}"),
        }
    }
}

/// Lexer for the contents inside `${...}`
fn var_lexer<'src>() -> impl Parser<
    'src,
    OffsetInput<'src>,
    Vec<InnerSpan<Token<'src>>>,
    extra::Err<Rich<'src, char, OffsetSpan>>,
> {
    let dot = just('.').to(Token::Dot);

    // Identifier: alphanumeric + underscore (no dots - those are separate tokens)
    let ident = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(Token::Ident)
        .labelled("identifier");

    let token = choice((dot, ident));

    token
        .map_with(|tok, e| (tok, e.span()))
        .repeated()
        .collect()
}

// ============================================================================
// Parser for variable contents (tokenized)
// ============================================================================

/// The parsed result of a variable reference
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedVariable<'src> {
    /// A config reference like `config.c8y.url` - stores the key part `c8y.url`
    Config(Vec<&'src str>),
    /// A template variable like `topic` - stores the variable name
    Variable(&'src str),
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

/// Parser for variable contents - either `config.some.key` or `varname`
fn var_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, ParsedVariable<'src>, extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>>
       + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    let config_ref = just(Token::Ident("config"))
        .ignore_then(just(Token::Dot))
        .ignore_then(dotted_path())
        .map(ParsedVariable::Config)
        .labelled("config reference (e.g. 'config.c8y.url')");

    let template_var = select! { Token::Ident(s) => s }
        .then_ignore(end())
        .map(ParsedVariable::Variable)
        .labelled("variable name (no dots allowed)");

    choice((config_ref, template_var)).labelled("variable")
}

/// Parse the contents of a `${...}` block
fn parse_var_contents(contents: &str, offset: usize) -> Result<ParsedVariable<'_>, ExpandError> {
    let (tokens, lex_errs) = var_lexer()
        .parse(contents.with_context(offset))
        .into_output_errors();

    if let Some(e) = lex_errs.into_iter().next() {
        return Err(ExpandError {
            message: format!("Invalid character in variable: {e}"),
            help: None,
            span: e.span().into_range(),
        });
    }

    let tokens = tokens.expect("tokens should exist if no errors");
    let len = contents.len();
    let eoi = offset + len;

    let (parsed, parse_errs) = var_parser()
        .parse(
            tokens
                .as_slice()
                .map(OffsetSpan::new(0, eoi..eoi), |(t, s)| (t, s)),
        )
        .into_output_errors();

    if let Some(e) = parse_errs.into_iter().next() {
        // Check if this looks like a dotted path without the config. prefix
        let has_dots = tokens.iter().any(|(t, _)| matches!(t, Token::Dot));
        let starts_with_ident = matches!(tokens.first(), Some((Token::Ident(_), _)));

        let (message, help, span) = if has_dots && starts_with_ident {
            // User likely forgot the config. prefix
            (
                format!("Unknown variable '{contents}'"),
                Some(format!("You might have meant 'config.{contents}'")),
                Some(offset..offset + len),
            )
        } else {
            (e.to_string(), None, None)
        };

        return Err(ExpandError {
            message,
            help,
            span: span.unwrap_or_else(|| e.span().into_range()),
        });
    }

    Ok(parsed.expect("parsed should exist if no errors"))
}

// ============================================================================
// Template component types and outer parser
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateComponent<'src> {
    /// A config reference like `${config.c8y.url}` - stores just `c8y.url`
    Config(String),
    /// A template variable like `${topic}` - stores just `topic`
    Variable(&'src str),
    /// Literal text
    Text(&'src str),
}

/// Parser for `${...}` - returns the contents and span of the contents
fn var_block<'src>() -> impl Parser<
    'src,
    OffsetInput<'src>,
    (&'src str, OffsetSpan),
    extra::Err<Rich<'src, char, OffsetSpan>>,
> + Clone {
    just("${")
        .ignore_then(
            any()
                .and_is(just('}').not())
                .repeated()
                .to_slice()
                .map_with(|s, e| (s, e.span())),
        )
        .then_ignore(just('}').labelled("closing '}'"))
        .labelled("variable reference (e.g. '${config.c8y.url}' or '${topic}')")
}

/// Parser for a complete template string like `something/${config.key}/else`
/// This finds `${...}` blocks and literal text, but defers parsing of variable
/// contents to the lexer/parser.
fn template_parser<'src>() -> impl Parser<
    'src,
    OffsetInput<'src>,
    Vec<(RawTemplateComponent<'src, OffsetSpan>, OffsetSpan)>,
    extra::Err<Rich<'src, char, OffsetSpan>>,
> + Clone {
    choice((
        var_block().map(|(contents, contents_span)| RawTemplateComponent::Variable {
            contents,
            contents_span,
        }),
        any()
            .and_is(just('$').not())
            .repeated()
            .at_least(1)
            .to_slice()
            .map(RawTemplateComponent::Text),
    ))
    .map_with(|tok, e| (tok, e.span()))
    .repeated()
    .collect()
}

/// Raw template component before variable contents are parsed
#[derive(Debug, Clone)]
enum RawTemplateComponent<'src, S> {
    /// A `${...}` block - contents need to be parsed
    Variable {
        contents: &'src str,
        contents_span: S,
    },
    /// Literal text
    Text(&'src str),
}

/// Parse a template string, returning components with their spans
pub fn parse_template(
    src: &toml::Spanned<String>,
) -> Result<Vec<(TemplateComponent<'_>, OffsetSpan)>, ExpandError> {
    // The input span includes the quotes in the original toml string, so bump by 1
    let offset = src.span().start + 1;

    let (raw_components, errs) = template_parser()
        .parse(src.get_ref().with_context::<OffsetSpan>(offset))
        .into_output_errors();

    if let Some(e) = errs.into_iter().next() {
        return Err(ExpandError {
            message: e.to_string(),
            help: None,
            span: e.span().into_range(),
        });
    }

    let raw_components = raw_components.expect("components should exist if no errors");

    // Convert raw components to final components by parsing variable contents
    let mut components = Vec::with_capacity(raw_components.len());
    for (raw, span) in raw_components {
        let component = match raw {
            RawTemplateComponent::Text(text) => TemplateComponent::Text(text),
            RawTemplateComponent::Variable {
                contents,
                contents_span,
            } => {
                // contents_span.start() is already absolute due to OffsetSpan
                let parsed = parse_var_contents(contents, contents_span.start())?;
                match parsed {
                    ParsedVariable::Config(parts) => TemplateComponent::Config(parts.join(".")),
                    ParsedVariable::Variable(name) => TemplateComponent::Variable(name),
                }
            }
        };
        components.push((component, span));
    }

    Ok(components)
}

/// Parse a config reference like `${config.c8y.url}`
pub fn parse_config_reference<T>(
    src: &toml::Spanned<String>,
) -> Result<ConfigReference<T>, ExpandError> {
    // The input span includes the quotes in the original toml string, so bump by 1
    let offset = src.span().start + 1;
    let input = src.get_ref();

    // First, parse the outer `${...}` structure
    let (raw_components, errs) = template_parser()
        .parse(input.as_str().with_context::<OffsetSpan>(offset))
        .into_output_errors();

    if let Some(e) = errs.into_iter().next() {
        // Improve error message for unclosed braces
        let message = if e.to_string().contains("closing '}'") {
            "config reference must end with }".to_string()
        } else {
            e.to_string()
        };
        return Err(ExpandError {
            message,
            help: None,
            span: e.span().into_range(),
        });
    }

    let raw_components = raw_components.expect("components should exist if no errors");

    // Expect exactly one variable component
    if raw_components.len() != 1 {
        return Err(ExpandError {
            message: "expected config reference".into(),
            help: Some("Use format: ${config.key}".into()),
            span: offset..(offset + input.len()),
        });
    }

    let (raw, _span) = raw_components.into_iter().next().unwrap();

    match raw {
        RawTemplateComponent::Variable {
            contents,
            contents_span,
        } => {
            // contents_span.start() already includes the offset
            let parsed = parse_var_contents(contents, contents_span.start());
            match parsed {
                Ok(ParsedVariable::Config(parts)) => {
                    let key = parts.join(".");
                    // The key span should point to the key part inside ${config.KEY}
                    // contents_span points to the full contents "config.key"
                    // We want to point to just "key", which starts after "config."
                    let key_start = contents_span.start() + "config.".len();
                    let key_end = contents_span.end();
                    Ok(ConfigReference(
                        toml::Spanned::new(key_start..key_end, key),
                        PhantomData,
                    ))
                }
                Ok(ParsedVariable::Variable(_)) | Err(_) => Err(ExpandError {
                    message: "expected config reference".into(),
                    help: Some("Use format: ${config.key}".into()),
                    span: contents_span.into_range(),
                }),
            }
        }
        RawTemplateComponent::Text(_text) => Err(ExpandError {
            message: "expected config reference".into(),
            help: Some("Use format: ${config.key}".into()),
            span: offset..(offset + input.len()),
        }),
    }
}

/// Expand a template that only contains config references (no template variables)
pub fn expand_config_template(
    src: &toml::Spanned<String>,
    config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> Result<String, ExpandError> {
    let components = parse_template(src)?;
    let mut result = String::new();

    for (component, span) in components {
        match component {
            TemplateComponent::Text(text) => result.push_str(text),
            TemplateComponent::Config(key) => {
                let value =
                    super::super::expand_config_key(&key, config, cloud_profile, span.into())?;
                result.push_str(&value);
            }
            TemplateComponent::Variable(var) => {
                return Err(ExpandError {
                    message: format!("Unknown variable '{var}'"),
                    help: Some(format!("Config references should use 'config.{var}'")),
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
    pub loop_var_name: &'a str,
    pub loop_var_value: &'a str,
}

/// Expand a template that can contain both config references and a template variable
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
            TemplateComponent::Config(key) => {
                let value =
                    super::super::expand_config_key(&key, ctx.tedge, cloud_profile, span.into())?;
                result.push_str(&value);
            }
            TemplateComponent::Variable(var) => {
                if var == ctx.loop_var_name {
                    result.push_str(ctx.loop_var_value);
                } else {
                    return Err(ExpandError {
                        message: format!("Unknown variable '{var}'"),
                        help: Some(format!(
                            "Did you mean '{}' or 'config.{}'?",
                            ctx.loop_var_name, var
                        )),
                        span: span.into(),
                    });
                }
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
        assert_eq!(components[0].0, TemplateComponent::Config("c8y.url".into()));
    }

    #[test]
    fn parses_template_variable() {
        let input = toml_spanned("${topic}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Variable("topic"));
    }

    #[test]
    fn parses_mixed_template() {
        let input = toml_spanned("prefix/${config.c8y.bridge.topic_prefix}/${topic}/suffix");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 5);
        assert_eq!(components[0].0, TemplateComponent::Text("prefix/"));
        assert_eq!(
            components[1].0,
            TemplateComponent::Config("c8y.bridge.topic_prefix".into())
        );
        assert_eq!(components[2].0, TemplateComponent::Text("/"));
        assert_eq!(components[3].0, TemplateComponent::Variable("topic"));
        assert_eq!(components[4].0, TemplateComponent::Text("/suffix"));
    }

    #[test]
    fn config_template_rejects_template_variables() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${topic}");
        let result = expand_config_template(&input, &config, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown variable"));
        assert_eq!(extract_toml_span(&input, err.span), "${topic}");
    }

    #[test]
    fn config_template_rejects_non_alphanumeric_characters() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${test@me}");
        let result = expand_config_template(&input, &config, None);
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
            loop_var_name: "topic",
            loop_var_value: "my-topic",
        };
        let result = expand_loop_template(&toml_spanned("s/uc/${topic}"), &ctx, None).unwrap();
        assert_eq!(result, "s/uc/my-topic");
    }

    #[test]
    fn template_template_rejects_unknown_variables() {
        let config = TEdgeConfig::load_toml_str("");
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_name: "topic",
            loop_var_value: "my-topic",
        };
        let result = expand_loop_template(&toml_spanned("${unknown}"), &ctx, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unknown variable"));
        assert!(err.help.as_ref().unwrap().contains("topic")); // suggests the loop var
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
    fn span_points_to_template_variable() {
        let input = toml_spanned("${topic}");
        let components = parse_template(&input).unwrap();
        assert_eq!(components.len(), 1);

        let (_, span) = &components[0];
        assert_eq!(extract_toml_span(&input, span.into_range()), "${topic}");
    }

    #[test]
    fn spans_in_mixed_template_point_to_correct_components() {
        let input = toml_spanned("prefix/${config.key}/${var}/suffix");
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
        // "${var}"
        assert_eq!(
            extract_toml_span(&input, components[3].1.into_range()),
            "${var}"
        );
        // "/suffix"
        assert_eq!(
            extract_toml_span(&input, components[4].1.into_range()),
            "/suffix"
        );
    }

    #[test]
    fn error_offset_for_unknown_variable_at_start() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("${unknown}/rest");
        let err = expand_config_template(&input, &config, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "${unknown}");
    }

    #[test]
    fn error_offset_for_unknown_variable_in_middle() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("prefix/${unknown}/suffix");
        let err = expand_config_template(&input, &config, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "${unknown}");
    }

    #[test]
    fn error_offset_for_invalid_config_key() {
        let config = TEdgeConfig::load_toml_str("");
        let input = toml_spanned("prefix/${config.invalid.key}/suffix");
        let err = expand_config_template(&input, &config, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "${config.invalid.key}");
    }

    #[test]
    fn error_offset_for_unknown_loop_variable() {
        let config = TEdgeConfig::load_toml_str("");
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_name: "topic",
            loop_var_value: "value",
        };
        let input = toml_spanned("start/${wrong}/end");
        let err = expand_loop_template(&input, &ctx, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "${wrong}");
    }

    #[test]
    fn error_offset_with_multiple_variables_points_to_failing_one() {
        let config = TEdgeConfig::load_toml_str("");
        let ctx = TemplateContext {
            tedge: &config,
            loop_var_name: "topic",
            loop_var_value: "value",
        };
        // First variable is valid, second is invalid
        let input = toml_spanned("${topic}/${unknown}");
        let err = expand_loop_template(&input, &ctx, None).unwrap_err();

        assert_eq!(extract_toml_span(&input, err.span), "${unknown}");
    }
}
