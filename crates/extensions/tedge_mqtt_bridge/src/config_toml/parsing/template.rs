//! Chumsky-based parser for template strings
//!
//! Templates can contain:
//! - `${config.some.key}` - config variable references
//! - `${varname}` - template variables (for template rules)
//! - literal text

use crate::config_toml::ExpandError;

use chumsky::prelude::*;
use tedge_config::tedge_toml::ProfileName;
use tedge_config::TEdgeConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateComponent<'src> {
    /// A config reference like `${config.c8y.url}` - stores just `c8y.url`
    Config(&'src str),
    /// A template variable like `${topic}` - stores just `topic`
    Variable(&'src str),
    /// Literal text
    Text(&'src str),
}

/// Parser for `${config.some.key}` - returns the key part (e.g., `some.key`)
fn config_var<'src>(
) -> impl Parser<'src, &'src str, &'src str, extra::Err<Rich<'src, char, SimpleSpan>>> {
    just("${config.")
        .ignore_then(
            any()
                .and_is(just('}').not())
                .repeated()
                .at_least(1)
                .to_slice(),
        )
        .then_ignore(just('}'))
}

/// Parser for `${varname}` - returns the variable name (no dots allowed)
fn template_var<'src>(
) -> impl Parser<'src, &'src str, &'src str, extra::Err<Rich<'src, char, SimpleSpan>>> {
    just("${")
        .ignore_then(
            any()
                .and_is(one_of("${}.").not())
                .repeated()
                .at_least(1)
                .to_slice(),
        )
        .then_ignore(just('}'))
}

/// Parser for a complete template string like `something/${config.key}/else`
fn template_parser<'src>() -> impl Parser<
    'src,
    &'src str,
    Vec<(TemplateComponent<'src>, SimpleSpan)>,
    extra::Err<Rich<'src, char, SimpleSpan>>,
> {
    choice((
        config_var().map(TemplateComponent::Config),
        template_var().map(TemplateComponent::Variable),
        any()
            .and_is(just('$').not())
            .repeated()
            .at_least(1)
            .to_slice()
            .map(TemplateComponent::Text),
    ))
    .map_with(|tok, e| (tok, e.span()))
    .repeated()
    .collect()
}

/// Parse a template string, returning components with their spans
pub fn parse_template(src: &str) -> Result<Vec<(TemplateComponent<'_>, SimpleSpan)>, ExpandError> {
    let (components, errs) = template_parser().parse(src).into_output_errors();

    if let Some(e) = errs.into_iter().next() {
        return Err(ExpandError {
            message: e.to_string(),
            help: None,
            span: e.span().into_range(),
        });
    }

    Ok(components.expect("components should exist if no errors"))
}

/// Expand a template that only contains config references (no template variables)
pub fn expand_config_template(
    src: &toml::Spanned<String>,
    config: &TEdgeConfig,
    cloud_profile: Option<&ProfileName>,
) -> Result<String, ExpandError> {
    let components = parse_template(src.get_ref())?;
    let mut result = String::new();

    for (component, span) in components {
        let span = (span.start + src.span().start + 1)..(span.end + src.span().start + 1);
        match component {
            TemplateComponent::Text(text) => result.push_str(text),
            TemplateComponent::Config(key) => {
                let value = super::super::expand_config_key(key, config, cloud_profile, span)?;
                result.push_str(&value);
            }
            TemplateComponent::Variable(var) => {
                return Err(ExpandError {
                    message: format!("Unknown variable '{var}'"),
                    help: Some(format!("Config references should use 'config.{var}'")),
                    span,
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
    let components = parse_template(src.get_ref())?;
    let mut result = String::new();

    for (component, span) in components {
        let span = (span.start + src.span().start + 1)..(span.end + src.span().start + 1);
        match component {
            TemplateComponent::Text(text) => result.push_str(text),
            TemplateComponent::Config(key) => {
                let value = super::super::expand_config_key(key, ctx.tedge, cloud_profile, span)?;
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
                        span,
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
        let components = parse_template("hello world").unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Text("hello world"));
    }

    #[test]
    fn parses_config_reference() {
        let components = parse_template("${config.c8y.url}").unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Config("c8y.url"));
    }

    #[test]
    fn parses_template_variable() {
        let components = parse_template("${topic}").unwrap();
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].0, TemplateComponent::Variable("topic"));
    }

    #[test]
    fn parses_mixed_template() {
        let components =
            parse_template("prefix/${config.c8y.bridge.topic_prefix}/${topic}/suffix").unwrap();
        assert_eq!(components.len(), 5);
        assert_eq!(components[0].0, TemplateComponent::Text("prefix/"));
        assert_eq!(
            components[1].0,
            TemplateComponent::Config("c8y.bridge.topic_prefix")
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
        let input = "${config.c8y.url}";
        let components = parse_template(input).unwrap();
        assert_eq!(components.len(), 1);

        let (_, span) = &components[0];
        assert_eq!(&input[span.into_range()], "${config.c8y.url}");
    }

    #[test]
    fn span_points_to_template_variable() {
        let input = "${topic}";
        let components = parse_template(input).unwrap();
        assert_eq!(components.len(), 1);

        let (_, span) = &components[0];
        assert_eq!(&input[span.into_range()], "${topic}");
    }

    #[test]
    fn spans_in_mixed_template_point_to_correct_components() {
        let input = "prefix/${config.key}/${var}/suffix";
        let components = parse_template(input).unwrap();
        assert_eq!(components.len(), 5);

        // "prefix/"
        assert_eq!(&input[components[0].1.into_range()], "prefix/");
        // "${config.key}"
        assert_eq!(&input[components[1].1.into_range()], "${config.key}");
        // "/"
        assert_eq!(&input[components[2].1.into_range()], "/");
        // "${var}"
        assert_eq!(&input[components[3].1.into_range()], "${var}");
        // "/suffix"
        assert_eq!(&input[components[4].1.into_range()], "/suffix");
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
