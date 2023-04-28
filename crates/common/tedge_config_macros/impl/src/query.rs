use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::VecDeque;
use syn::parse_quote;

use crate::input::{ConfigurableField, FieldOrGroup};

pub fn generate_writable_keys(items: &[FieldOrGroup]) -> TokenStream {
    let paths = configuration_paths_from(items);
    let readable_keys = keys_enum(parse_quote!(ReadableKey), paths.iter());
    let writable_keys = keys_enum(
        parse_quote!(WritableKey),
        paths.iter().filter(|path| is_read_write(path)),
    );

    quote! {
        #readable_keys
        #writable_keys

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

fn keys_enum<'a>(
    type_name: syn::Ident,
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> TokenStream {
    let (configuration_string, variant_name): (Vec<_>, Vec<_>) = variants
        .map(|segments| {
            (
                segments
                    .iter()
                    .map(|variant| variant.ident().to_string())
                    .collect::<Vec<_>>()
                    .join("."),
                syn::Ident::new(
                    &segments
                        .iter()
                        .map(|segment| segment.ident().to_string().to_upper_camel_case())
                        .collect::<String>(),
                    segments.iter().last().unwrap().ident().span(),
                ),
            )
        })
        .unzip();
    let simplified_configuration_string = configuration_string.iter().map(|s| s.replace('.', "_"));

    quote! {
        #[derive(Copy, Clone)]
        #[non_exhaustive]
        #[allow(unused)]
        pub enum #type_name {
            #(
                #variant_name,
            )*
        }

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
                    _ => Err(format!("unknown key: '{}'", value)),
                }
            }
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
