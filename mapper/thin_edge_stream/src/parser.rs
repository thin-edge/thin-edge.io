use crate::measurement::GroupedMeasurementConsumer;
use chrono::DateTime;
use nom::character::complete::*;
use nom::combinator::*;
use nom::error::ErrorKind;
use nom::number::complete::*;
use nom::IResult;

pub fn thin_edge_json<C, E, D>(input: &str, mut consumer: C) -> Result<D, ParseError<E>>
where
    C: GroupedMeasurementConsumer<Error = E, Data = D>,
    E: std::error::Error,
{
    consumer.start()?;

    let (input, _) = multispace0(input).map_err(ParseError::new)?;
    let input = collect_measurements(input, &mut consumer)?;
    let (_, _) = eof(input).map_err(ParseError::new)?;

    Ok(consumer.end()?)
}

#[derive(thiserror::Error, Debug)]
pub enum ParseError<E>
where
    E: std::error::Error + 'static,
{
    #[error("Fail to parse thin-edge-json input: {cause:?}")]
    ParseError { cause: String },

    #[error(transparent)]
    BuildError(#[from] E),
}

impl<E> ParseError<E>
where
    E: std::error::Error,
{
    fn new(from: nom::Err<nom::error::Error<&str>>) -> ParseError<E> {
        let cause = format!("{}", from);
        ParseError::ParseError { cause }
    }

    fn date_parsing_error(from: chrono::ParseError) -> Self {
        let cause = format!("{}", from);
        ParseError::ParseError { cause }
    }
}

enum MeasurementKind {
    String,
    Number,
    Group,
}

fn collect_measurements<'a, C, E, D>(
    input: &'a str,
    consumer: &mut C,
) -> Result<&'a str, ParseError<E>>
where
    C: GroupedMeasurementConsumer<Error = E, Data = D>,
    E: std::error::Error,
{
    let (input, ()) = separator('{')(input).map_err(ParseError::new)?;
    let (input, is_end) = peek_group_end(input).map_err(ParseError::new)?;

    let mut loop_input = input;
    let mut more_measurements = !is_end;
    while more_measurements {
        let input = loop_input;

        let (input, name) = quoted_string(input).map_err(ParseError::new)?;
        let (input, ()) = separator(':')(input).map_err(ParseError::new)?;
        let (input, kind) = peek_measurement_kind(input).map_err(ParseError::new)?;
        match kind {
            MeasurementKind::String => {
                let (input, value) = quoted_string(input).map_err(ParseError::new)?;
                let timestamp =
                    DateTime::parse_from_rfc3339(value).map_err(ParseError::date_parsing_error)?;
                consumer.timestamp(timestamp)?;
                loop_input = input;
            }
            MeasurementKind::Number => {
                let (input, value) = number(input).map_err(ParseError::new)?;
                consumer.measurement(name, value)?;
                loop_input = input;
            }
            MeasurementKind::Group => {
                consumer.start_group(name)?;
                let input = collect_measurements(input, consumer)?;
                consumer.end_group()?;
                loop_input = input;
            }
        }

        let (input, is_end) = peek_group_end(loop_input).map_err(ParseError::new)?;
        loop_input = input;
        more_measurements = !is_end;
    }

    let (input, ()) = separator('}')(loop_input).map_err(ParseError::new)?;
    Ok(input)
}

fn peek_group_end(input: &str) -> IResult<&str, bool> {
    let (input, token) = peek(one_of("}\","))(input)?;
    match token {
        '}' => Ok((input, true)),
        ',' => {
            // Let's consume the comma
            let (input, ()) = separator(',')(input)?;
            Ok((input, false))
        }
        _ => Ok((input, false)),
    }
}

fn peek_measurement_kind(input: &str) -> IResult<&str, MeasurementKind> {
    let (input, token) = peek(one_of("\"{"))(input)?;
    match token {
        '{' => Ok((input, MeasurementKind::Group)),
        '"' => Ok((input, MeasurementKind::String)),
        _ => Ok((input, MeasurementKind::Number)),
    }
}

/// Parse a given char and any following whitespaces.
///
/// ```
/// # use thin_edge_stream::parser::*;
/// # use nom::error::ErrorKind;
/// assert_eq!(separator('{')("{   xyz"), Ok(("xyz",())));
/// assert_eq!(separator('{')("{xyz"), Ok(("xyz",())));
/// assert_eq!(separator('{')("}   xyz"), Err(parse_error("}   xyz", ErrorKind::Char)));
/// assert_eq!(separator('{')("  {xyz"), Err(parse_error("  {xyz", ErrorKind::Char)));
/// ```
pub fn separator(sep: char) -> impl Fn(&str) -> IResult<&str, ()> {
    move |input| {
        let (input, _) = char(sep)(input)?;
        let (input, _) = multispace0(input)?;
        Ok((input, ()))
    }
}

/// Parse a quoted string returning that string without the quotes and consuming trailing whitespaces.
///
/// ```
/// # use thin_edge_stream::parser::*;
/// # use nom::error::ErrorKind;
/// assert_eq!(quoted_string(r#""foo" xyz"#), Ok(("xyz","foo")));
/// assert_eq!(quoted_string(r#""" xyz"#), Ok(("xyz","")));
/// assert_eq!(quoted_string("xyz"), Err(parse_error("xyz", ErrorKind::Char)));
/// assert_eq!(quoted_string(r#"   "foo" xyz"#), Err(parse_error(r#"   "foo" xyz"#, ErrorKind::Char)));
/// assert_eq!(quoted_string(r#""foo xyz"#), Err(parse_error(r#""foo xyz"#, ErrorKind::Eof)));
/// ```
pub fn quoted_string(input0: &str) -> IResult<&str, &str> {
    let (input, _) = char('"')(input0)?;
    match input.find('"') {
        Some(end) => {
            let (name, input) = input.split_at(end);
            let (input, _) = char('"')(input)?;
            let (input, _) = multispace0(input)?;
            Ok((input, name))
        }
        None => Err(parse_error(input0, ErrorKind::Eof)),
    }
}

/// Parse a float, consuming any trailing whitespaces.
///
/// ```
/// # use thin_edge_stream::parser::*;
/// # use nom::error::ErrorKind;
/// assert_eq!(number(r"1.0 xyz"), Ok(("xyz",1.0)));
/// assert_eq!(number(r"1 xyz"), Ok(("xyz",1.0)));
/// assert_eq!(number(r"1.e2 xyz"), Ok(("xyz",100.0)));
/// assert_eq!(number("xyz"), Err(parse_error("xyz", ErrorKind::Float)));
/// assert_eq!(number(r"   1.0 xyz"), Err(parse_error(r"   1.0 xyz", ErrorKind::Float)));
/// ```
pub fn number(input: &str) -> IResult<&str, f64> {
    let (input, num) = double(input)?;
    let (input, _) = multispace0(input)?;
    Ok((input, num))
}

/// Parse a measurement.
///
/// ```
/// # use thin_edge_stream::parser::*;
/// # use nom::error::ErrorKind;
/// assert_eq!(measurement(r#""foo": 1.1 xyz"#), Ok(("xyz",("foo",1.1))));
/// ```
pub fn measurement(input: &str) -> IResult<&str, (&str, f64)> {
    let (input, name) = quoted_string(input)?;
    let (input, ()) = separator(':')(input)?;
    let (input, value) = number(input)?;
    Ok((input, (name, value)))
}

pub fn parse_error(input: &str, kind: ErrorKind) -> nom::Err<nom::error::Error<&str>> {
    nom::Err::Error(nom::error::Error::new(input, kind))
}
