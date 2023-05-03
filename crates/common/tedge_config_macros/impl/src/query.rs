use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::VecDeque;
use syn::parse_quote;
use syn::Field;

use crate::input::ConfigurableField;
use crate::input::FieldOrGroup;

pub fn generate_writable_keys(items: &[FieldOrGroup]) -> TokenStream {
    let paths = configuration_paths_from(items);
    let (readonly_variant, write_error) = paths
        .iter()
        .filter_map(|field| {
            Some((
                variant_name(field),
                field
                    .back()?
                    .field()?
                    .read_only()?
                    .readonly
                    .write_error
                    .as_str(),
            ))
        })
        .unzip::<_, _, Vec<_>, Vec<_>>();
    let readable_args = configuration_strings(paths.iter());
    let readonly_args = configuration_strings(paths.iter().filter(|path| !is_read_write(path)));
    let writable_args = configuration_strings(paths.iter().filter(|path| is_read_write(path)));
    let readable_keys = keys_enum(parse_quote!(ReadableKey), &readable_args);
    let readonly_keys = keys_enum(parse_quote!(ReadonlyKey), &readonly_args);
    let writable_keys = keys_enum(parse_quote!(WritableKey), &writable_args);
    let fromstr_readable = generate_fromstr_readable(parse_quote!(ReadableKey), &readable_args);
    let fromstr_readonly = generate_fromstr_readable(parse_quote!(ReadonlyKey), &readonly_args);
    let fromstr_writable = generate_fromstr_writable(parse_quote!(WritableKey), &writable_args);

    quote! {
        #readable_keys
        #readonly_keys
        #writable_keys
        #fromstr_readable
        #fromstr_readonly
        #fromstr_writable

        impl ReadonlyKey {
            fn write_error(self) -> &'static str {
                match self {
                    #(
                        Self::#readonly_variant => #write_error,
                    )*
                }
            }
        }

        fn replace_aliases(key: String) -> String {
            use ::once_cell::sync::Lazy;
            use ::std::borrow::Cow;
            use ::std::collections::HashMap;
            use ::doku::*;

            static ALIASES: Lazy<HashMap<Cow<'static, str>, Cow<'static, str>>> = Lazy::new(|| {
                let ty = TEdgeConfigReader::ty();
                let TypeKind::Struct { fields, transparent: false } = ty.kind else { panic!("Expected struct but got {:?}", ty.kind) };
                let Fields::Named { fields } = fields else { panic!("Expected named fields but got {:?}", fields)};
                struct_field_aliases(None, &fields)
            });

            ALIASES
                .get(&Cow::Borrowed(key.as_str()))
                .map(|c| c.clone().into_owned())
                .unwrap_or(key)
        }
    }
}

fn configuration_strings<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<String>, Vec<syn::Ident>) {
    variants
        .map(|segments| {
            (
                segments
                    .iter()
                    .map(|variant| variant.name())
                    .collect::<Vec<_>>()
                    .join("."),
                variant_name(segments),
            )
        })
        .unzip()
}

fn generate_fromstr(
    type_name: syn::Ident,
    (configuration_string, variant_name): &(Vec<String>, Vec<syn::Ident>),
    error_case: syn::Arm,
) -> TokenStream {
    let simplified_configuration_string = configuration_string.iter().map(|s| s.replace('.', "_"));

    quote! {
        impl ::std::str::FromStr for #type_name {
            type Err = String;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                    #(
                        #simplified_configuration_string => {
                            if (value != #configuration_string) {
                                ::tracing::warn!("The key '{}' is deprecated. Use '{}' instead.", value, #configuration_string);
                            }
                            Ok(Self::#variant_name)
                        },
                    )*
                    #error_case
                }
            }
        }
    }
}

fn generate_fromstr_readable<'a>(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<syn::Ident>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! { _ => Err(format!("unknown key: '{}'", value)) },
    )
}

// TODO test the error messages actually appear
fn generate_fromstr_writable<'a>(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<syn::Ident>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! {
            _ => if let Ok(key) = <ReadonlyKey as ::std::str::FromStr>::from_str(value) {
                Err(key.write_error().to_owned())
            } else {
                Err(format!("unknown key: '{}'", value))
            },
        },
    )
}

fn keys_enum<'a>(
    type_name: syn::Ident,
    (configuration_string, variant_name): &(Vec<String>, Vec<syn::Ident>),
) -> TokenStream {
    quote! {
        #[derive(Copy, Clone)]
        #[non_exhaustive]
        #[allow(unused)]
        pub enum #type_name {
            #(
                #variant_name,
            )*
        }

        impl #type_name {
            pub fn as_str(self) -> &'static str {
                match self {
                    #(
                        Self::#variant_name => #configuration_string,
                    )*
                }
            }
        }
    }
}

fn variant_name(segments: &VecDeque<&FieldOrGroup>) -> syn::Ident {
    syn::Ident::new(
        &segments
            .iter()
            .map(|segment| segment.name().to_upper_camel_case())
            .collect::<String>(),
        segments.iter().last().unwrap().ident().span(),
    )
}

/// Generates a list of the toml paths for each of the keys in the provided
/// configuration
fn configuration_paths_from(items: &[FieldOrGroup]) -> Vec<VecDeque<&FieldOrGroup>> {
    let mut res = vec![];
    for item in items {
        match item {
            FieldOrGroup::Field(_) => res.push(VecDeque::from([item])),
            FieldOrGroup::Group(group) => {
                for mut fields in configuration_paths_from(&group.contents) {
                    fields.push_front(item);
                    res.push(fields);
                }
            }
        }
    }
    res
}

/// Checks if the field for the given path is read write
fn is_read_write(path: &VecDeque<&FieldOrGroup>) -> bool {
    matches!(
        path.back(), // the field
        Some(FieldOrGroup::Field(ConfigurableField::ReadWrite(_))),
    )
}
