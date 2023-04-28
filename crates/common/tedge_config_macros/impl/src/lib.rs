//! This crate implements the macro for `tedge_config_macros` and should not be used directly.

use heck::ToUpperCamelCase;
use optional_error::OptionalError;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};

mod dto;
mod input;
mod query;
mod reader;
mod utils;

#[doc(hidden)]
pub fn generate_configuration(tokens: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: input::Configuration = syn::parse2(tokens)?;

    let mut error = OptionalError::default();
    let fields_with_keys = input
        .groups
        .iter()
        .flat_map(|group| match group {
            input::FieldOrGroup::Group(group) => unfold_group(Vec::new(), group),
            input::FieldOrGroup::Field(field) => {
                error.combine(syn::Error::new(
                    field.ident().span(),
                    "top level fields are not supported",
                ));
                vec![]
            }
        })
        .collect::<Vec<_>>();
    error.try_throw()?;

    let example_tests = fields_with_keys
        .iter()
        .filter_map(|(key, field)| Some((key, field.read_write()?)))
        .flat_map(|(key, field)| {
            let ty = &field.ty;
            field.examples.iter().enumerate().map(move |(n, example)| {
                let name = quote::format_ident!(
                    "example_value_can_be_deserialized_for_{}_example_{n}",
                    key.join("_")
                );
                let span = example.span();
                let example = example.as_ref();
                let expect_message = format!(
                    "Example value {example:?} for '{}' could not be deserialized",
                    key.join(".")
                );
                quote_spanned! {span=>
                    #[test]
                    fn #name() {
                        #example.parse::<#ty>().expect(#expect_message);
                    }
                }
            })
        })
        .collect::<Vec<_>>();

    let dto = dto::generate(
        proc_macro2::Ident::new("TEdgeConfigDto", Span::call_site()),
        &input.groups,
    );

    let reader = reader::try_generate(
        proc_macro2::Ident::new("TEdgeConfigReader", Span::call_site()),
        &input.groups,
    )?;

    let enums = query::generate_writable_keys(&input.groups);

    Ok(quote! {
        #(#example_tests)*
        #dto
        #reader
        #enums
    })
}

fn unfold_group(
    mut name: Vec<String>,
    group: &input::ConfigurationGroup,
) -> Vec<(Vec<String>, &input::ConfigurableField)> {
    let mut output = Vec::new();
    name.push(group.ident.to_string());
    for field_or_group in &group.contents {
        match field_or_group {
            input::FieldOrGroup::Field(field) => {
                let mut name = name.clone();
                name.push(
                    field
                        .rename()
                        .map(<_>::to_owned)
                        .unwrap_or_else(|| field.ident().to_string()),
                );
                output.push((name, field))
            }
            input::FieldOrGroup::Group(group) => {
                output.append(&mut unfold_group(name.clone(), group));
            }
        }
    }

    output
}

fn prefixed_type_name(
    start: &proc_macro2::Ident,
    group: &input::ConfigurationGroup,
) -> proc_macro2::Ident {
    quote::format_ident!(
        "{start}{}",
        group.ident.to_string().to_upper_camel_case(),
        span = group.ident.span()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO should these move to parse
    #[test]
    fn parse_basic_configuration_with_attributes() {
        assert!(generate_configuration(quote! {
            device: {
                /// The id of the device
                #[tedge_config(readonly)]
                id: String,
                /// The key path
                #[tedge_config(example = "test")]
                #[tedge_config(example = "tes2")]
                key_path: Utf8Path,
            }
        })
        .is_ok());
    }

    #[test]
    fn parse_nested_groups() {
        assert!(generate_configuration(quote! {
            device: {
                nested: {
                    #[tedge_config(rename = "type")]
                    ty: String,
                },
            },
        })
        .is_ok());
    }
}
