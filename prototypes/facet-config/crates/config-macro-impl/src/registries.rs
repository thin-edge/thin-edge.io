use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeSet;

use crate::input::{Configuration, FieldDefault, FieldOrGroup};

pub fn generate_registries(config: &Configuration) -> TokenStream {
    let defaults_fn = generate_defaults(config);
    let read_only_fn = generate_read_only_keys(config);
    let aliases_fn = generate_aliases(config);
    let examples_fn = generate_examples(config);

    // Each entry generates a `fn build_NAME() -> ReturnType` that calls
    // `register_fn::<T>(&mut r)` for every leaf type in the config.
    // To add a new action, add one line here.
    let type_registries = [generate_type_registry(
        config,
        "build_registry",
        "AppendRemoveRegistry",
        "register_append_remove",
    )];

    quote! {
        #defaults_fn
        #(#type_registries)*
        #read_only_fn
        #aliases_fn
        #examples_fn
    }
}

fn generate_defaults(config: &Configuration) -> TokenStream {
    let mut entries = Vec::new();
    collect_defaults(&config.groups, "", &mut entries);

    quote! {
        fn build_defaults(config_dir: &std::path::Path) -> DefaultsRegistry {
            DefaultsRegistry::new(vec![
                #(#entries)*
            ])
            .expect("invalid defaults registry")
        }
    }
}

fn collect_defaults(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    prefix: &str,
    entries: &mut Vec<TokenStream>,
) {
    for item in items {
        match item {
            FieldOrGroup::Field(f) => {
                let key = dotted_key(prefix, &f.config_name());
                if let Some(default) = &f.default {
                    let entry = match default {
                        FieldDefault::Value(v) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::Value(#v.into()),
                            },
                        },
                        FieldDefault::Function(func) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::Function(#func),
                            },
                        },
                        FieldDefault::FromKey(source) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::FromKey(#source),
                            },
                        },
                        FieldDefault::FromOptionalKey(source) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::FromOptionalKey(#source),
                            },
                        },
                        FieldDefault::FromConfigDir(rel_path) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::Value(
                                    config_dir.join(#rel_path)
                                        .to_string_lossy()
                                        .into_owned(),
                                ),
                            },
                        },
                        FieldDefault::FromRoot(root_key) => quote! {
                            FieldDefault {
                                key: #key,
                                spec: DefaultSpec::FromRoot(#root_key),
                            },
                        },
                    };
                    entries.push(entry);
                }
            }
            FieldOrGroup::Group(g) => {
                let group_prefix = dotted_key(prefix, &g.config_name());
                collect_defaults(&g.contents, &group_prefix, entries);
            }
        }
    }
}

/// Generates a function that builds a `TypeActionRegistry`-based registry by
/// calling a registration function for each leaf type in the config.
///
/// To support a new action, add one call to this in `generate_registries`.
fn generate_type_registry(
    config: &Configuration,
    fn_name: &str,
    return_type: &str,
    register_fn: &str,
) -> TokenStream {
    let mut types = BTreeSet::new();
    collect_leaf_types(&config.groups, &mut types);

    let fn_name: syn::Ident = syn::parse_str(fn_name).unwrap();
    let return_type: syn::Type = syn::parse_str(return_type).unwrap();
    let register_fn: syn::Path = syn::parse_str(register_fn).unwrap();

    let register_calls: Vec<TokenStream> = types
        .iter()
        .map(|ty_str| {
            let ty: syn::Type = syn::parse_str(ty_str).unwrap();
            quote! { #register_fn::<#ty>(&mut r); }
        })
        .collect();

    quote! {
        fn #fn_name() -> #return_type {
            let mut r = #return_type::new();
            #(#register_calls)*
            r
        }
    }
}

fn collect_leaf_types(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    types: &mut BTreeSet<String>,
) {
    for item in items {
        match item {
            FieldOrGroup::Field(f) => {
                let ty = &f.ty;
                let ty_str = quote::quote!(#ty).to_string();
                // Normalize whitespace for dedup
                let normalized: String = ty_str.split_whitespace().collect::<Vec<_>>().join(" ");
                types.insert(normalized);
            }
            FieldOrGroup::Group(g) => {
                collect_leaf_types(&g.contents, types);
            }
        }
    }
}

fn generate_read_only_keys(config: &Configuration) -> TokenStream {
    let mut keys = Vec::new();
    collect_read_only(&config.groups, "", &mut keys);

    quote! {
        fn build_read_only_keys() -> ReadOnlyKeys {
            ReadOnlyKeys::new([#(#keys),*])
        }
    }
}

fn collect_read_only(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    prefix: &str,
    keys: &mut Vec<String>,
) {
    for item in items {
        match item {
            FieldOrGroup::Field(f) => {
                if f.readonly {
                    keys.push(dotted_key(prefix, &f.config_name()));
                }
            }
            FieldOrGroup::Group(g) => {
                let group_prefix = dotted_key(prefix, &g.config_name());
                collect_read_only(&g.contents, &group_prefix, keys);
            }
        }
    }
}

fn generate_aliases(config: &Configuration) -> TokenStream {
    let mut aliases = Vec::new();
    collect_aliases(&config.groups, "", &mut aliases);

    quote! {
        fn build_aliases() -> KeyAliases {
            KeyAliases::new(vec![#(#aliases)*])
        }
    }
}

fn collect_aliases(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    prefix: &str,
    aliases: &mut Vec<TokenStream>,
) {
    for item in items {
        match item {
            FieldOrGroup::Field(f) => {
                if let Some(old_key) = &f.deprecated_key {
                    let new_key = dotted_key(prefix, &f.config_name());
                    aliases.push(quote! {
                        DeprecatedKey { old: #old_key, new: #new_key },
                    });
                }
            }
            FieldOrGroup::Group(g) => {
                let group_prefix = dotted_key(prefix, &g.config_name());
                collect_aliases(&g.contents, &group_prefix, aliases);
            }
        }
    }
}

fn generate_examples(config: &Configuration) -> TokenStream {
    let mut entries = Vec::new();
    collect_examples(&config.groups, "", &mut entries);

    quote! {
        fn build_examples() -> std::collections::HashMap<&'static str, &'static [&'static str]> {
            std::collections::HashMap::from([
                #(#entries)*
            ])
        }
    }
}

fn collect_examples(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    prefix: &str,
    entries: &mut Vec<TokenStream>,
) {
    for item in items {
        match item {
            FieldOrGroup::Field(f) => {
                if !f.examples.is_empty() {
                    let key = dotted_key(prefix, &f.config_name());
                    let examples = &f.examples;
                    entries.push(quote! {
                        (#key, [#(#examples),*].as_slice()),
                    });
                }
            }
            FieldOrGroup::Group(g) => {
                let group_prefix = dotted_key(prefix, &g.config_name());
                collect_examples(&g.contents, &group_prefix, entries);
            }
        }
    }
}

fn dotted_key(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[track_caller]
    fn assert_eq(actual: &TokenStream, expected: &TokenStream) {
        let actual: syn::File = syn::parse2(actual.clone()).unwrap();
        let expected: syn::File = syn::parse2(expected.clone()).unwrap();
        pretty_assertions::assert_eq!(
            prettyplease::unparse(&actual),
            prettyplease::unparse(&expected),
        );
    }

    #[test]
    fn static_default_generates_value_spec() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(default(value = "1883"))]
                    port: u16,
                },
            }
        );
        let generated = generate_defaults(&input);
        let expected: TokenStream = parse_quote! {
            fn build_defaults(config_dir: &std::path::Path) -> DefaultsRegistry {
                DefaultsRegistry::new(vec![
                    FieldDefault {
                        key: "mqtt.port",
                        spec: DefaultSpec::Value("1883".into()),
                    },
                ])
                .expect("invalid defaults registry")
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn from_key_default_generates_from_key_spec() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    device: {
                        #[tedge_config(default(from_key = "device.cert_path"))]
                        cert_path: String,
                    },
                },
            }
        );
        let generated = generate_defaults(&input);
        let expected: TokenStream = parse_quote! {
            fn build_defaults(config_dir: &std::path::Path) -> DefaultsRegistry {
                DefaultsRegistry::new(vec![
                    FieldDefault {
                        key: "c8y.device.cert_path",
                        spec: DefaultSpec::FromKey("device.cert_path"),
                    },
                ])
                .expect("invalid defaults registry")
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn from_config_dir_generates_join_expression() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_config_dir = "device-certs/cert.pem"))]
                    cert_path: String,
                },
            }
        );
        let generated = generate_defaults(&input);
        let expected: TokenStream = parse_quote! {
            fn build_defaults(config_dir: &std::path::Path) -> DefaultsRegistry {
                DefaultsRegistry::new(vec![
                    FieldDefault {
                        key: "device.cert_path",
                        spec: DefaultSpec::Value(
                            config_dir.join("device-certs/cert.pem")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    },
                ])
                .expect("invalid defaults registry")
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn readonly_fields_collected() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    proxy: {
                        #[tedge_config(readonly)]
                        port: u16,
                    },
                },
            }
        );
        let generated = generate_read_only_keys(&input);
        let expected: TokenStream = parse_quote! {
            fn build_read_only_keys() -> ReadOnlyKeys {
                ReadOnlyKeys::new(["c8y.proxy.port"])
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn deprecated_keys_collected() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(deprecated_key = "mqtt.external.port")]
                    port: u16,
                },
            }
        );
        let generated = generate_aliases(&input);
        let expected: TokenStream = parse_quote! {
            fn build_aliases() -> KeyAliases {
                KeyAliases::new(vec![
                    DeprecatedKey { old: "mqtt.external.port", new: "mqtt.port" },
                ])
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn examples_collected_for_fields() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    #[tedge_config(example = "your-tenant.cumulocity.com")]
                    url: String,
                },
                device: {
                    #[tedge_config(example = "my-device", example = "AINA123")]
                    id: String,
                    name: String,
                },
            }
        );
        let generated = generate_examples(&input);
        let expected: TokenStream = parse_quote! {
            fn build_examples() -> std::collections::HashMap<&'static str, &'static [&'static str]> {
                std::collections::HashMap::from([
                    ("c8y.url", ["your-tenant.cumulocity.com"].as_slice()),
                    ("device.id", ["my-device", "AINA123"].as_slice()),
                ])
            }
        };
        assert_eq(&generated, &expected);
    }
}
