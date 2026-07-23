//! Generates the runtime information used to apply defaults and perform
//! config operations.
//!
//! External groups contribute their own information under the key where they
//! are reused. Read-only markers, deprecated key aliases, and example values
//! are now facet attributes on the DTO fields (see `dto.rs`) and discovered
//! at runtime via shape-tree walks instead of codegen tables.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use std::collections::BTreeMap;
use syn::spanned::Spanned;

use crate::input::FieldDefault;
use crate::model::{ExternalModel, Model};

pub fn generate_schema(model: &Model) -> TokenStream {
    let reader_ident = &model.root.reader_ident;
    let dto_ident = &model.root.dto_ident;
    let defaults_fn = generate_defaults(model);
    let register_types_fn = generate_register_types(model);

    quote! {
        impl ConfigSchema for #reader_ident {
            type Dto = #dto_ident;

            #defaults_fn
            #register_types_fn
        }
    }
}

fn generate_defaults(model: &Model) -> TokenStream {
    let entries: Vec<TokenStream> = model
        .root
        .fields()
        .into_iter()
        .filter_map(|f| {
            let key = &f.key;
            let default = f.field.default.as_ref()?;
            let spec = match default {
                FieldDefault::Value(v) => quote! { DefaultSpec::Value(#v.into()) },
                FieldDefault::Function(func) => quote! { DefaultSpec::Function(#func) },
                FieldDefault::FromKey(source) => quote! { DefaultSpec::FromKey(#source.into()) },
                FieldDefault::FromOptionalKey(source) => {
                    quote! { DefaultSpec::FromOptionalKey(#source.into()) }
                }
                FieldDefault::FromConfigDir(rel_path) => quote! {
                    DefaultSpec::Value(
                        config_dir.join(#rel_path)
                            .to_string_lossy()
                            .into_owned(),
                    )
                },
                FieldDefault::FromRoot(root_key) => quote! { DefaultSpec::FromRoot(#root_key) },
                FieldDefault::FromKeyVia(via) => {
                    let source = &via.key;
                    let function = &via.function;
                    let ty = &f.field.ty;
                    // Bind the function to the field's type so a bad
                    // signature is reported at the caller's attribute.
                    let adapter = quote_spanned! {function.span()=>
                        |source: &str| derive_to_string::<#ty>(#function, source)
                    };
                    quote! {
                        DefaultSpec::FromKeyVia {
                            key: #source.into(),
                            function: #adapter,
                        }
                    }
                }
            };
            Some(quote! {
                FieldDefault {
                    key: #key.into(),
                    spec: #spec,
                },
            })
        })
        .collect();

    let body = chained_defaults(&entries, model.root.externals());

    quote! {
        fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
            #body
        }
    }
}

fn generate_register_types(model: &Model) -> TokenStream {
    // Keep the first spelling so errors retain its source span.
    let mut types = BTreeMap::new();
    for f in model.root.fields() {
        let ty = &f.field.ty;
        let ty_str = quote::quote!(#ty).to_string();
        let normalized: String = ty_str.split_whitespace().collect::<Vec<_>>().join(" ");
        types.entry(normalized).or_insert(ty);
    }

    // Keep missing trait errors on the field declaration.
    let register_calls = types.values().map(|ty| {
        let register_fn = syn::Ident::new("register_append_remove", ty.span());
        quote_spanned! {ty.span()=> #register_fn::<#ty>(registry); }
    });

    let forwarded = model.root.externals().into_iter().map(|ext| {
        let ty = &ext.ext.ty;
        quote_spanned! {ty.span()=> <#ty as ConfigSchema>::register_types(registry); }
    });

    quote! {
        fn register_types(registry: &mut AppendRemoveRegistry) {
            #(#register_calls)*
            #(#forwarded)*
        }
    }
}

fn chained_defaults(
    entries: &[TokenStream],
    externals: Vec<&ExternalModel>,
) -> TokenStream {
    if externals.is_empty() {
        return quote! { vec![ #(#entries)* ] };
    }
    let extends = externals.into_iter().map(|ext| {
        let prefix = &ext.prefix;
        let ty = &ext.ext.ty;
        quote_spanned! {ty.span()=>
            items.extend(prefix_defaults(#prefix, <#ty as ConfigSchema>::defaults(config_dir)));
        }
    });
    quote! {
        let mut items: Vec<FieldDefault> = vec![ #(#entries)* ];
        #(#extends)*
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Configuration;
    use crate::test_utils::{ident_starts, position_of};
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

    fn defaults_fn(config: &Configuration) -> TokenStream {
        generate_defaults(&Model::new(config))
    }

    #[test]
    fn impl_block_links_reader_to_dto() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    port: u16,
                },
            }
        );
        let generated = generate_schema(&Model::new(&input));
        let expected: TokenStream = parse_quote! {
            impl ConfigSchema for MapperConfig {
                type Dto = MapperConfigDto;

                fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                    vec![]
                }
                fn register_types(registry: &mut AppendRemoveRegistry) {
                    register_append_remove::<u16>(registry);
                }
            }
        };
        assert_eq(&generated, &expected);
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
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "mqtt.port".into(),
                        spec: DefaultSpec::Value("1883".into()),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn function_default_generates_function_spec() {
        let input: Configuration = parse_quote!(
            Test {
                run: {
                    #[tedge_config(default(function = "generated_value"))]
                    stamp: String,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "run.stamp".into(),
                        spec: DefaultSpec::Function(generated_value),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn from_optional_key_default_generates_from_optional_key_spec() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    #[tedge_config(default(from_optional_key = "c8y.url"))]
                    http: String,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "c8y.http".into(),
                        spec: DefaultSpec::FromOptionalKey("c8y.url".into()),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn from_root_default_generates_from_root_spec() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_root = "device.cert_path"))]
                    cert_path: String,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "device.cert_path".into(),
                        spec: DefaultSpec::FromRoot("device.cert_path"),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
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
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "c8y.device.cert_path".into(),
                        spec: DefaultSpec::FromKey("device.cert_path".into()),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn from_key_via_default_generates_from_key_via_spec() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_key_via(
                        key = "device.cert_path",
                        function = "device_id_from_cert"
                    )))]
                    id: String,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "device.id".into(),
                        spec: DefaultSpec::FromKeyVia {
                            key: "device.cert_path".into(),
                            function: |source: &str| derive_to_string::<String>(device_id_from_cert, source),
                        },
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn from_key_via_adapter_returns_the_field_type() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(default(from_key_via(
                        key = "mqtt.port",
                        function = "external_port"
                    )))]
                    external_port: u16,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "mqtt.external_port".into(),
                        spec: DefaultSpec::FromKeyVia {
                            key: "mqtt.port".into(),
                            function: |source: &str| derive_to_string::<u16>(external_port, source),
                        },
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
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
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                vec![
                    FieldDefault {
                        key: "device.cert_path".into(),
                        spec: DefaultSpec::Value(
                            config_dir.join("device-certs/cert.pem")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    },
                ]
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn external_schema_defaults_are_remapped_under_the_mount_key() {
        let input: Configuration = parse_quote!(
            Mapper {
                #[tedge_config(default(value = "1883"))]
                port: u16,

                device: extern MapperDeviceConfig,
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                let mut items: Vec<FieldDefault> = vec![
                    FieldDefault {
                        key: "port".into(),
                        spec: DefaultSpec::Value("1883".into()),
                    },
                ];
                items.extend(prefix_defaults(
                    "device",
                    <MapperDeviceConfig as ConfigSchema>::defaults(config_dir),
                ));
                items
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn nested_external_schema_is_remapped_under_the_full_dotted_key() {
        let input: Configuration = parse_quote!(
            Mapper {
                c8y: {
                    device: extern MapperDeviceConfig,
                },
            }
        );
        let expected: TokenStream = parse_quote! {
            fn defaults(config_dir: &std::path::Path) -> Vec<FieldDefault> {
                let mut items: Vec<FieldDefault> = vec![];
                items.extend(prefix_defaults(
                    "c8y.device",
                    <MapperDeviceConfig as ConfigSchema>::defaults(config_dir),
                ));
                items
            }
        };
        assert_eq(&defaults_fn(&input), &expected);
    }

    #[test]
    fn external_schema_leaf_types_are_registered_via_the_schema() {
        let input: Configuration = parse_quote!(
            Test {
                port: u16,
                device: extern MapperDeviceConfig,
            }
        );
        let generated = generate_register_types(&Model::new(&input));
        let expected: TokenStream = parse_quote! {
            fn register_types(registry: &mut AppendRemoveRegistry) {
                register_append_remove::<u16>(registry);
                <MapperDeviceConfig as ConfigSchema>::register_types(registry);
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn register_calls_span_the_field_type() {
        let src = "Test {
    mqtt: {
        port: u16,
    },
    c8y: {
        url: String,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate_register_types(&Model::new(&input));
        // Types are ordered by name, so String comes before u16
        assert_eq!(
            ident_starts(&generated, "register_append_remove"),
            vec![position_of(src, "String"), position_of(src, "u16")],
        );
        assert_eq!(
            ident_starts(&generated, "u16"),
            vec![position_of(src, "u16")],
        );
    }

    #[test]
    fn duplicate_types_span_the_first_occurrence() {
        let src = "Test {
    mqtt: {
        port: u16,
        bind_port: u16,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate_register_types(&Model::new(&input));
        assert_eq!(
            ident_starts(&generated, "u16"),
            vec![position_of(src, "u16")],
        );
    }
}
