//! This crate implements the macro for `tedge_config_macros` and should not be used directly.

use crate::input::FieldDefault;
use heck::ToUpperCamelCase;
use optional_error::OptionalError;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;

mod dto;
mod error;
mod input;
mod namegen;
mod optional_error;
mod query;
mod reader;

#[doc(hidden)]
pub fn generate_configuration(tokens: TokenStream) -> Result<TokenStream, syn::Error> {
    let input: input::Configuration = syn::parse2(tokens)?;

    let mut error = OptionalError::default();
    let fields_with_keys = input
        .groups
        .iter()
        .flat_map(|group| match group {
            input::FieldOrGroup::Group(group) => unfold_group(Vec::new(), group),
            input::FieldOrGroup::Multi(group) => unfold_group(Vec::new(), group),
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
            let ty = field.from.as_ref().unwrap_or(&field.ty);
            field.examples.iter().enumerate().map(move |(n, example)| {
                let name = quote::format_ident!(
                    "example_value_can_be_deserialized_for_{}_example_{n}",
                    key.join("_").replace('-', "_")
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

    let fromstr_default_tests = fields_with_keys
        .iter()
        .filter_map(|(key, field)| Some((key, field.read_write()?)))
        .filter_map(|(key, field)| {
            let ty = field.from.as_ref().unwrap_or(&field.ty);
            if let FieldDefault::FromStr(default) = &field.default {
                let name = quote::format_ident!(
                    "default_value_can_be_deserialized_for_{}",
                    key.join("_").replace('-', "_")
                );
                let span = default.span();
                let expect_message = format!(
                    "Default value {default:?} for '{}' could not be deserialized",
                    key.join("."),
                );
                Some(quote_spanned! {span=>
                    #[test]
                    fn #name() {
                        #default.parse::<#ty>().expect(#expect_message);
                    }
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let reader_name = proc_macro2::Ident::new("TEdgeConfigReader", Span::call_site());
    let dto_doc_comment = format!(
        "A data-transfer object, designed for reading and writing to
        `tedge.toml`
\n\
        All the configurations inside this are optional to represent whether
        the value is or isn't configured in the TOML file. Any defaults are
        populated when this is converted to [{reader_name}] (via
        [from_dto]({reader_name}::from_dto)).
\n\
        For simplicity when using this struct, only the fields are optional.
        Any configuration groups (e.g. `device`, `c8y`, `mqtt.external`) are
        always present. Groups that have no value set will be omitted in the
        serialized output to avoid polluting `tedge.toml`."
    );

    let dto = dto::generate(
        proc_macro2::Ident::new("TEdgeConfigDto", Span::call_site()),
        &input.groups,
        &dto_doc_comment,
    );

    let reader_doc_comment = "A struct to read configured values from, designed to be accessed only
        via an immutable borrow
\n\
        The configurations inside this struct are optional only if the field
        does not have a default value configured. This ensures that thin-edge
        code only needs to handle possible errors where a field may not be
        set.
\n\
        Where fields are optional, they are stored using [OptionalConfig] to
        produce a descriptive error message that directs the user to set the
        relevant key.";
    let reader = reader::try_generate(reader_name, &input.groups, reader_doc_comment)?;

    let enums = query::generate_writable_keys(&input.groups);

    Ok(quote! {
        #(#example_tests)*
        #(#fromstr_default_tests)*
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
            input::FieldOrGroup::Multi(group) => {
                name.push("*".to_owned());
                output.append(&mut unfold_group(name.clone(), group));
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
                #[tedge_config(readonly(write_error = "Device id is derived from the certificate and cannot be written to", function = "device_id"))]
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

    #[test]
    fn serde_rename_is_not_allowed() {
        assert!(generate_configuration(quote! {
            device: {
                #[serde(rename = "type")]
                ty: String,
            },
        })
        .is_err());
    }

    #[test]
    fn can_contain_hyphen_separated_fields() {
        generate_configuration(quote! {
            device: {
                #[tedge_config(rename = "type")]
                ty: String,

                #[tedge_config(rename = "hyphen-separated-field", example = "hsf")]
                hyphen_separated_field: String
            },
        })
        .unwrap();
    }

    #[test]
    fn can_contain_multi_fields() {
        generate_configuration(quote! {
            #[multi]
            c8y: {
                url: String
            },
        })
        .unwrap();
    }

    #[test]
    fn error_message_suggests_fix_in_case_of_invalid_value() {
        assert_eq!(generate_configuration(quote! {
            http: {
                #[tedge_config(default(value = Ipv4Addr::LOCALHOST))]
                address: Ipv4Addr,
            },
        })
                       .unwrap_err()
                       .to_string(),
                   "Unexpected expression, `default(value = ...)` expects a literal.\n\
            Perhaps you want to use `#[tedge_config(default(variable = \"Ipv4Addr::LOCALHOST\"))]`?");
    }
}
