//! Chumsky-based parser for bridge config conditions
//!
//! This module uses a two-stage lexer/parser approach:
//! 1. Lexer: converts input string into a sequence of tokens with spans
//! 2. Parser: parses tokens into a Condition expression

use crate::config_toml::parsing::OffsetSpan;
use crate::config_toml::ConfigReference;

use super::super::AuthMethod;
use super::super::Condition;
use super::super::ExpandError;
use chumsky::input::ValueInput;
use chumsky::input::WithContext;
use chumsky::prelude::*;
use std::fmt;
use std::marker::PhantomData;
use toml::Spanned;

pub type Span<T> = (T, OffsetSpan);
type OffsetInput<'src> = WithContext<OffsetSpan, &'src str>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token<'src> {
    /// `${` - start of a variable reference
    VarStart,
    /// `.` - dot separator for variable references
    Dot,
    /// `}` - end of a variable reference
    VarEnd,
    /// An operator like `==`, `!=`, `=`
    Op(&'src str),
    /// An identifier like `config`, `connection`, `auth_method`
    Ident(&'src str),
    /// A string literal (contents without quotes), e.g. `certificate` from `'certificate'`
    String(&'src str),
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Token::VarStart => write!(f, "${{"),
            Token::VarEnd => write!(f, "}}"),
            Token::Op(op) => write!(f, "{op}"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::String(s) => write!(f, "'{s}'"),
            Token::Dot => write!(f, "."),
        }
    }
}

fn lexer<'src>(
) -> impl Parser<'src, OffsetInput<'src>, Vec<Span<Token<'src>>>, extra::Err<Rich<'src, char, OffsetSpan>>>
{
    let var_start = just("${").to(Token::VarStart);
    let var_end = just('}').to(Token::VarEnd);
    let dot = just('.').to(Token::Dot);

    // Operators: any combination of `!` and `=` (e.g., `==`, `!=`, `=`)
    // We don't currently support != but may wish to in the future, and users may try this
    // so recognising it but explicitly rejecting it is sensible
    let op = one_of("!=")
        .repeated()
        .at_least(1)
        .to_slice()
        .map(Token::Op)
        .labelled("comparison operator (e.g. '==')");

    // String literal: 'contents'
    let string = just('\'')
        .ignore_then(any().and_is(just('\'').not()).repeated().to_slice())
        .then_ignore(just('\''))
        .map(Token::String)
        .labelled("single-quoted string literal");

    // Identifier: alphanumeric + underscore (no dots - those are separate tokens)
    let ident = any()
        .filter(|c: &char| c.is_alphanumeric() || *c == '_')
        .repeated()
        .at_least(1)
        .to_slice()
        .map(Token::Ident)
        .labelled("identifier");

    // Order matters: try more specific patterns first
    let token = choice((var_start, var_end, dot, op, string, ident));

    token
        .map_with(|tok, e| (tok, e.span()))
        .padded() // skip whitespace between tokens
        .repeated()
        .collect()
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
        .labelled("dotted path")
}

/// Parser for `${config.some.key}` - a boolean config reference
fn config_condition<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Condition, extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    just(Token::Op("!"))
        .or_not()
        .then_ignore(just(Token::VarStart))
        .then_ignore(just(Token::Ident("config")))
        .then_ignore(just(Token::Dot))
        .then(dotted_path().spanned())
        .then_ignore(just(Token::VarEnd))
        .map(move |(negation, (parts, span)): (_, Span<Vec<&str>>)| {
            let key = parts.join(".");
            let target = negation.is_none();
            Condition::Is(
                target,
                ConfigReference(Spanned::new(span.into_range(), key), PhantomData),
            )
        })
        .labelled("config reference")
}

/// Parser for `${connection.auth_method} == 'value'`
fn auth_condition<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Condition, extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    let auth_method_var = just(Token::VarStart)
        .ignore_then(just(Token::Ident("connection")))
        .ignore_then(just(Token::Dot))
        .ignore_then(just(Token::Ident("auth_method")))
        .then_ignore(just(Token::VarEnd));

    let eq_op = just(Token::Op("=="));

    let string_value = select! { Token::String(s) => s }.labelled("string value");

    auth_method_var
        .ignore_then(eq_op)
        .ignore_then(string_value.try_map(|s: &str, span| {
            s.parse::<AuthMethod>().map_err(|_| {
                Rich::custom(
                    span,
                    format!("expected 'certificate' or 'password', got '{s}'"),
                )
            })
        }))
        .map(Condition::AuthMethod)
        .labelled("auth method condition")
}

/// Main condition parser
fn condition_parser<'tokens, 'src: 'tokens, I>(
) -> impl Parser<'tokens, I, Condition, extra::Err<Rich<'tokens, Token<'src>, OffsetSpan>>> + Clone
where
    I: ValueInput<'tokens, Token = Token<'src>, Span = OffsetSpan>,
{
    choice((config_condition(), auth_condition())).labelled("condition")
}

impl std::str::FromStr for AuthMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "certificate" => Ok(AuthMethod::Certificate),
            "password" => Ok(AuthMethod::Password),
            other => Err(format!(
                "unknown auth method: '{other}', expected 'certificate' or 'password'"
            )),
        }
    }
}

/// Parse a condition string, formatting errors with span information for ariadne
pub fn parse_condition_with_error(
    input: &Spanned<impl AsRef<str>>,
) -> Result<Spanned<Condition>, Vec<ExpandError>> {
    let src = input.get_ref().as_ref();
    // input span includes the quotes in the original toml string, so bump this
    // by 1 to get the start of the input
    let offset = input.span().start + 1;

    let (tokens, lex_errs) = lexer().parse(src.with_context(offset)).into_output_errors();

    if !lex_errs.is_empty() {
        return Err(lex_errs
            .into_iter()
            .map(|e| ExpandError {
                message: format!("Lexer error: {e}"),
                span: e.span().into_range(),
                help: None,
            })
            .collect());
    }

    let tokens = tokens.expect("tokens should exist if no errors");

    let len = src.len();
    let eoi = offset + len;

    let (ast, parse_errs) = condition_parser()
        .parse(
            tokens
                .as_slice()
                .map(OffsetSpan::new(0, eoi..eoi), |(t, s)| (t, s)),
        )
        .into_output_errors();

    if !parse_errs.is_empty() {
        return Err(parse_errs
            .into_iter()
            .map(|e| ExpandError {
                message: format!("{e}"),
                span: e.span().into_range(),
                help: None,
            })
            .collect());
    }

    let condition = ast.expect("ast should exist if no errors");
    Ok(Spanned::new(input.span(), condition))
}

#[cfg(test)]
mod tests {
    use crate::config_toml::test_helpers::*;

    use super::*;

    #[test]
    fn lexer_tokenizes_config_reference() {
        let input = "${config.c8y.enabled}";
        let (tokens, errs) = lexer().parse(input.with_context(0)).into_output_errors();
        assert!(errs.is_empty(), "Lexer errors: {errs:?}");
        let tokens: Vec<_> = tokens.unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::VarStart,
                Token::Ident("config"),
                Token::Dot,
                Token::Ident("c8y"),
                Token::Dot,
                Token::Ident("enabled"),
                Token::VarEnd,
            ]
        );
    }

    #[test]
    fn lexer_tokenizes_auth_condition() {
        let input = "${connection.auth_method} == 'certificate'";
        let (tokens, errs) = lexer().parse(input.with_context(0)).into_output_errors();
        assert!(errs.is_empty(), "Lexer errors: {errs:?}");
        let tokens: Vec<_> = tokens.unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::VarStart,
                Token::Ident("connection"),
                Token::Dot,
                Token::Ident("auth_method"),
                Token::VarEnd,
                Token::Op("=="),
                Token::String("certificate"),
            ]
        );
    }

    #[test]
    fn parses_config_reference() {
        let result = parse_condition_str("${config.c8y.mqtt_service.enabled}").unwrap();
        assert_eq!(
            result,
            Condition::Is(true, "${config.c8y.mqtt_service.enabled}".parse().unwrap(),)
        );
    }

    #[test]
    fn parses_negated_config_reference() {
        let result = parse_condition_str("!${config.c8y.mqtt_service.enabled}").unwrap();
        assert_eq!(
            result,
            Condition::Is(false, "${config.c8y.mqtt_service.enabled}".parse().unwrap(),)
        );
    }

    #[test]
    fn parses_auth_method_certificate() {
        let result = parse_condition_str("${connection.auth_method} == 'certificate'").unwrap();
        assert_eq!(result, Condition::AuthMethod(AuthMethod::Certificate));
    }

    #[test]
    fn parses_auth_method_password() {
        let result = parse_condition_str("${connection.auth_method} == 'password'").unwrap();
        assert_eq!(result, Condition::AuthMethod(AuthMethod::Password));
    }

    #[test]
    fn handles_whitespace_around_equals() {
        let result = parse_condition_str("${connection.auth_method}   ==   'password'").unwrap();
        assert_eq!(result, Condition::AuthMethod(AuthMethod::Password));
    }

    #[test]
    fn rejects_invalid_auth_method() {
        let result = parse_condition_str("${connection.auth_method} == 'invalid'");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str: String = err
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(
            err_str.contains("certificate") || err_str.contains("password"),
            "Error should mention valid options: {err_str}"
        );
    }

    #[test]
    fn rejects_missing_closing_brace() {
        let result = parse_condition_str("${config.some.key");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_variable_type() {
        let result = parse_condition_str("${unknown.variable}");
        assert!(result.is_err());
    }

    #[test]
    fn lexer_tokenizes_wrong_operator() {
        // Single `=` instead of `==` - should still tokenize
        // This allows us to give clearer error messages if someone inputs a slightly wrong operator
        let input = "${connection.auth_method} = 'password'";
        let (tokens, errs) = lexer().parse(input.with_context(0)).into_output_errors();
        assert!(errs.is_empty(), "Lexer errors: {errs:?}");
        let tokens: Vec<_> = tokens.unwrap().into_iter().map(|(t, _)| t).collect();
        assert_eq!(
            tokens,
            vec![
                Token::VarStart,
                Token::Ident("connection"),
                Token::Dot,
                Token::Ident("auth_method"),
                Token::VarEnd,
                Token::Op("="),
                Token::String("password"),
            ]
        );
    }

    #[test]
    fn error_for_wrong_operator() {
        // Single `=` instead of `==`
        let result = parse_condition_str("${connection.auth_method} = 'password'");
        assert!(result.is_err());
    }

    #[test]
    fn tokens_preserve_spans() {
        let input = "${config.key}";
        let (tokens, _) = lexer().parse(input.with_context(0)).into_output_errors();
        let tokens = tokens.unwrap();

        // Check that VarStart span is correct
        let (tok, span) = &tokens[0];
        assert_eq!(*tok, Token::VarStart);
        assert_eq!(&input[span.into_range()], "${");

        // Check that VarEnd span is correct
        let (tok, span) = &tokens[4];
        assert_eq!(*tok, Token::VarEnd);
        assert_eq!(&input[span.into_range()], "}");
    }

    // ========================================================================
    // Error message and span tests
    // ========================================================================

    #[test]
    fn error_span_for_invalid_auth_method_typo() {
        let input = toml_spanned("${connection.auth_method} == 'certifcate'");
        let err = parse_condition_with_error(&input).unwrap_err();

        assert_eq!(err.len(), 1);
        let ExpandError { message, span, .. } = &err[0];

        // Error message should mention valid options
        assert!(
            message.contains("certificate") || message.contains("password"),
            "Error should mention valid options: {message}"
        );

        // Span should highlight the typo'd string 'certifcate'
        let highlighted = extract_toml_span(&input, span.clone());
        assert_eq!(
            highlighted, "'certifcate'",
            "Span should highlight the invalid string literal, got: {highlighted}"
        );
    }

    #[test]
    fn error_span_for_unknown_variable_type() {
        let input = toml_spanned("${unknown.variable}");
        let err = parse_condition_with_error(&input).unwrap_err();

        assert_eq!(err.len(), 1);
        let ExpandError { message, span, .. } = &err[0];

        // Error message should say what was expected
        assert!(
            message.contains("config") && message.contains("connection"),
            "Error should mention valid variable types: {message}"
        );

        // Span should highlight 'unknown'
        let highlighted = extract_toml_span(&input, span.clone());
        assert_eq!(
            highlighted, "unknown",
            "Span should highlight the unknown identifier, got: {highlighted}"
        );
    }

    #[test]
    fn error_span_for_wrong_operator() {
        let input = toml_spanned("${connection.auth_method} = 'password'");
        let err = parse_condition_with_error(&input).unwrap_err();

        assert_eq!(err.len(), 1);
        let ExpandError { message, span, .. } = &err[0];

        // Error should mention expected '==' and what was found
        assert!(
            message.contains("'=='"),
            "Should mention expected '==': {message}"
        );

        // Span should highlight the `=` operator
        let highlighted = extract_toml_span(&input, span.clone());
        assert_eq!(
            highlighted, "=",
            "Span should highlight the wrong operator, got: {highlighted}"
        );
    }

    #[test]
    fn error_span_for_missing_value() {
        let input = toml_spanned("${connection.auth_method} ==");
        let err = parse_condition_with_error(&input).unwrap_err();

        assert_eq!(err.len(), 1);
        let ExpandError { message, span, .. } = &err[0];

        // Should indicate end of input and what was expected
        assert!(
            message.contains("end of input") && message.contains("string"),
            "Error should mention end of input and expected string: {message}"
        );

        // Span should be at end of input
        assert_eq!(
            extract_toml_span(&input, span.start..span.end + 1),
            "\"",
            "Span should point at the closing \" in toml"
        );
        assert_eq!(
            span.start,
            input.get_ref().len() + 1,
            "Span should be at end of input"
        );
    }

    #[test]
    fn error_span_for_unclosed_string() {
        let input = toml_spanned("${connection.auth_method} == 'certificate");
        let err = parse_condition_with_error(&input).unwrap_err();

        assert_eq!(err.len(), 1);
        let ExpandError { message, span, .. } = &err[0];

        // This should be a lexer error about unclosed string
        assert!(
            message.contains("Lexer") && message.contains("end of input"),
            "Should be a lexer error about end of input: {message}"
        );

        // Span should be at end of input
        assert_eq!(
            extract_toml_span(&input, span.start..span.end + 1),
            "\"",
            "Span should point at the closing \" in toml"
        );
        assert_eq!(
            span.start,
            input.get_ref().len() + 1,
            "Span should be at end of input"
        );
    }

    fn parse_condition_str(input: &str) -> Result<Condition, Vec<ExpandError>> {
        let spanned = toml_spanned(input);
        parse_condition_with_error(&spanned).map(|condition| condition.into_inner())
    }
}
