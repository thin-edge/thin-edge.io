use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use std::collections::VecDeque;
use syn::parse_quote;

use crate::error::extract_type_from_result;
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
    let readable_keys = keys_enum(parse_quote!(ReadableKey), &readable_args, "read from");
    let readonly_keys = keys_enum(
        parse_quote!(ReadOnlyKey),
        &readonly_args,
        "read from, but not written to,",
    );
    let writable_keys = keys_enum(parse_quote!(WritableKey), &writable_args, "written to");
    let fromstr_readable = generate_fromstr_readable(parse_quote!(ReadableKey), &readable_args);
    let fromstr_readonly = generate_fromstr_readable(parse_quote!(ReadOnlyKey), &readonly_args);
    let fromstr_writable = generate_fromstr_writable(parse_quote!(WritableKey), &writable_args);
    let read_string = generate_string_readers(&paths);
    let write_string = generate_string_writers(
        &paths
            .iter()
            .filter(|path| is_read_write(path))
            .cloned()
            .collect::<Vec<_>>(),
    );
    let (static_alias, updated_key) = deprecated_keys(paths.iter());

    quote! {
        #readable_keys
        #readonly_keys
        #writable_keys
        #fromstr_readable
        #fromstr_readonly
        #fromstr_writable
        #read_string
        #write_string

        #[derive(::thiserror::Error, Debug)]
        /// An error encountered when writing to a configuration value from a
        /// string
        pub enum WriteError {
            #[error("Failed to parse input")]
            ParseValue(#[from] Box<dyn ::std::error::Error + Send + Sync>),
        }

        impl ReadOnlyKey {
            fn write_error(self) -> &'static str {
                match self {
                    #(
                        Self::#readonly_variant => #write_error,
                    )*
                }
            }
        }

        #[derive(Debug, ::thiserror::Error)]
        /// An error encountered when parsing a configuration key from a string
        pub enum ParseKeyError {
            #[error("{}", .0.write_error())]
            ReadOnly(ReadOnlyKey),
            #[error("Unknown key: '{0}'")]
            Unrecognised(String),
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
                let mut aliases = struct_field_aliases(None, &fields);
                #(
                    if let Some(alias) = aliases.insert(Cow::Borrowed(#static_alias), Cow::Borrowed(ReadableKey::#updated_key.as_str())) {
                        panic!("Duplicate configuration alias for '{}'. It maps to both '{}' and '{}'. Perhaps you provided an incorrect `deprecated_key` for one of these configurations?", #static_alias, alias, ReadableKey::#updated_key.as_str());
                    }
                )*
                aliases
            });

            ALIASES
                .get(&Cow::Borrowed(key.as_str()))
                .map(|c| c.clone().into_owned())
                .unwrap_or(key)
        }

        fn warn_about_deprecated_key(deprecated_key: String, updated_key: &'static str) {
            use ::once_cell::sync::Lazy;
            use ::std::sync::Mutex;
            use ::std::collections::HashSet;

            static WARNINGS: Lazy<Mutex<HashSet<String>>> = Lazy::new(<_>::default);

            let warning = format!("The key '{}' is deprecated. Use '{}' instead.", deprecated_key, updated_key);
            if WARNINGS.lock().unwrap().insert(deprecated_key) {
                ::tracing::warn!("{}", warning);
            }
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

fn deprecated_keys<'a>(
    variants: impl Iterator<Item = &'a VecDeque<&'a FieldOrGroup>>,
) -> (Vec<&'a str>, Vec<syn::Ident>) {
    variants
        .flat_map(|segments| {
            segments
                .back()
                .unwrap()
                .field()
                .unwrap()
                .deprecated_keys()
                .map(|key| (key, variant_name(segments)))
        })
        .unzip()
}

fn generate_fromstr(
    type_name: syn::Ident,
    (configuration_string, variant_name): &(Vec<String>, Vec<syn::Ident>),
    error_case: syn::Arm,
) -> TokenStream {
    let simplified_configuration_string = configuration_string
        .iter()
        .map(|s| s.replace('.', "_"))
        .zip(variant_name.iter())
        .map(|(s, v)| quote_spanned!(v.span()=> #s));

    quote! {
        impl ::std::str::FromStr for #type_name {
            type Err = ParseKeyError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                // If we get an unreachable pattern, it means we have the same key twice
                #[deny(unreachable_patterns)]
                match replace_aliases(value.to_owned()).replace(".", "_").as_str() {
                    #(
                        #simplified_configuration_string => {
                            if (value != #configuration_string) {
                                warn_about_deprecated_key(value.to_owned(), #configuration_string);
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

fn generate_fromstr_readable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<syn::Ident>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! { _ => Err(ParseKeyError::Unrecognised(value.to_owned())) },
    )
}

// TODO test the error messages actually appear
fn generate_fromstr_writable(
    type_name: syn::Ident,
    fields: &(Vec<String>, Vec<syn::Ident>),
) -> TokenStream {
    generate_fromstr(
        type_name,
        fields,
        parse_quote! {
            _ => if let Ok(key) = <ReadOnlyKey as ::std::str::FromStr>::from_str(value) {
                Err(ParseKeyError::ReadOnly(key))
            } else {
                Err(ParseKeyError::Unrecognised(value.to_owned()))
            },
        },
    )
}

fn keys_enum(
    type_name: syn::Ident,
    (configuration_string, variant_name): &(Vec<String>, Vec<syn::Ident>),
    doc_fragment: &'static str,
) -> TokenStream {
    let as_str_example = variant_name
        .iter()
        .zip(configuration_string.iter())
        .map(|(ident, value)| format!("assert_eq!({type_name}::{ident}.as_str(), \"{value}\");\n"))
        .take(10)
        .collect::<Vec<_>>();
    let as_str_example = (!as_str_example.is_empty()).then(|| {
        quote! {
            /// ```compile_fail
            /// // This doctest is compile_fail because we have no way import the
            /// // current type, but the example is still valuable
            #(
                #[doc = #as_str_example]
            )*
            /// ```
        }
    });
    let type_name_str = type_name.to_string();

    quote! {
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        #[allow(unused)]
        #[doc = concat!("A key that can be *", #doc_fragment, "* the configuration\n\n")]
        #[doc = concat!("This can be converted to `&'static str` using [`", #type_name_str, "::as_str`], and")]
        #[doc = "parsed using [`FromStr`](::std::str::FromStr). The `FromStr` implementation also"]
        #[doc = "automatically emits warnings about deprecated keys. It also implements [Display](std::fmt::Display),"]
        #[doc = "so you can also use it in format strings."]
        pub enum #type_name {
            #(
                #[doc = concat!("`", #configuration_string, "`")]
                #variant_name,
            )*
        }

        impl #type_name {
            /// Converts this key to the canonical key used by `tedge config` and `tedge.toml`
            #as_str_example
            pub fn as_str(self) -> &'static str {
                match self {
                    #(
                        Self::#variant_name => #configuration_string,
                    )*
                }
            }

            /// Iterates through all the variants of this enum
            pub fn iter() -> impl Iterator<Item = Self> {
                [
                    #(
                        Self::#variant_name,
                    )*
                ].into_iter()
            }
        }

        impl ::std::fmt::Display for #type_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                self.as_str().fmt(f)
            }
        }
    }
}

fn generate_string_readers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let variant_names = paths.iter().map(variant_name);
    let arms = paths
        .iter()
        .zip(variant_names)
        .map(|(path, variant_name)| -> syn::Arm {
            let field = path
                .back()
                .expect("Path must have a back as it is nonempty")
                .field()
                .expect("Back of path is guaranteed to be a field");
            let segments = path.iter().map(|thing| thing.ident());
            if field.read_only().is_some() {
                if extract_type_from_result(field.ty()).is_some() {
                    parse_quote! {
                        // Probably where the compiler error appears
                        // TODO why do we need to unwrap
                        ReadableKey::#variant_name => Ok(self.#(#segments).*.try_read(self)?.to_string()),
                    }
                } else {
                    parse_quote! {
                        // Probably where the compiler error appears
                        // TODO why do we need to unwrap
                        ReadableKey::#variant_name => Ok(self.#(#segments).*.read(self).to_string()),
                    }
                }
            } else if field.has_guaranteed_default() {
                parse_quote! {
                    ReadableKey::#variant_name => Ok(self.#(#segments).*.to_string()),
                }
            } else {
                parse_quote! {
                    ReadableKey::#variant_name => Ok(self.#(#segments).*.or_config_not_set()?.to_string()),
                }
            }
        });
    quote! {
        impl TEdgeConfigReader {
            pub fn read_string(&self, key: ReadableKey) -> Result<String, ReadError> {
                match key {
                    #(#arms)*
                }
            }
        }
    }
}

fn generate_string_writers(paths: &[VecDeque<&FieldOrGroup>]) -> TokenStream {
    let variant_names = paths.iter().map(variant_name);
    let (update_arms, unset_arms): (Vec<syn::Arm>, Vec<syn::Arm>) = paths
        .iter()
        .zip(variant_names)
        .map(|(path, variant_name)| {
            let segments = path.iter().map(|thing| thing.ident()).collect::<Vec<_>>();

            (
                // TODO this should probably be spanned to the field type
                parse_quote! {
                    WritableKey::#variant_name => self.#(#segments).* = Some(value.parse().map_err(|e| WriteError::ParseValue(Box::new(e)))?),
                },
                parse_quote! {
                    WritableKey::#variant_name => self.#(#segments).* = None,
                }
            )
        }).unzip();
    quote! {
        impl TEdgeConfigDto {
            pub fn try_update_str(&mut self, key: WritableKey, value: &str) -> Result<(), WriteError> {
                match key {
                    #(#update_arms)*
                };
                Ok(())
            }

            pub fn unset_key(&mut self, key: WritableKey) {
                match key {
                    #(#unset_arms)*
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
    for item in items.iter().filter(|item| !item.reader().skip) {
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
