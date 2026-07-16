//! Generates the typed configuration returned to application code.
//!
//! Fields with guaranteed defaults are plain values. Other fields retain
//! their key so an unset value can produce a useful error when accessed.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

use crate::input::{ConfigField, FieldDefault};
use crate::model::{GroupModel, ItemModel, Model};

pub fn generate_reader(model: &Model) -> TokenStream {
    let structs = generate_group(&model.root);
    quote! { #(#structs)* }
}

fn generate_group(group: &GroupModel) -> Vec<TokenStream> {
    let mut nested = Vec::new();
    let mut fields = Vec::new();

    for item in &group.items {
        match item {
            ItemModel::Field(f) => fields.push(generate_reader_leaf(f.field)),
            ItemModel::Group(child) => {
                let ident = child.ident;
                let ty = &child.group.reader_ident;
                let doc_attrs = child.doc_attrs;
                fields.push(quote! {
                    #(#doc_attrs)*
                    pub #ident: #ty,
                });
                nested.extend(generate_group(&child.group));
            }
            ItemModel::External(ext) => {
                let ident = &ext.ext.ident;
                let ty = &ext.ext.ty;
                let doc_attrs = &ext.ext.doc_attrs;
                // Report an invalid external type at the caller's declaration.
                let field_ty = quote_spanned! {ty.span()=> #ty };
                fields.push(quote! {
                    #(#doc_attrs)*
                    pub #ident: #field_ty,
                });
            }
        }
    }

    let struct_ident = &group.reader_ident;
    let mut structs = vec![quote! {
        #[derive(Debug, ::facet::Facet)]
        #[facet(type_tag = "config_group")]
        pub struct #struct_ident {
            #(#fields)*
        }
    }];
    structs.extend(nested);
    structs
}

fn generate_reader_leaf(field: &ConfigField) -> TokenStream {
    let ident = &field.ident;
    let ty = &field.ty;
    let doc_attrs = &field.doc_attrs;

    let has_concrete_default = matches!(
        &field.default,
        Some(
            FieldDefault::Value(_)
                | FieldDefault::Function(_)
                | FieldDefault::FromConfigDir(_)
                | FieldDefault::FromKey(_)
        )
    );

    let mut extra_attrs = Vec::new();
    if let Some(rename) = &field.rename {
        extra_attrs.push(quote! { #[facet(rename = #rename)] });
    }

    if has_concrete_default {
        quote! {
            #(#doc_attrs)*
            #(#extra_attrs)*
            pub #ident: #ty,
        }
    } else {
        let field_ty = quote_spanned! {ty.span()=> OptionalConfig<#ty> };
        quote! {
            #(#doc_attrs)*
            #(#extra_attrs)*
            pub #ident: #field_ty,
        }
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

    fn generate(config: &Configuration) -> TokenStream {
        generate_reader(&Model::new(config))
    }

    #[test]
    fn field_with_default_is_concrete_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    #[tedge_config(default(value = "1883"))]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfig {
                pub mqtt: MqttConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct MqttConfig {
                pub port: u16,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn field_without_default_is_optional() {
        let input: Configuration = parse_quote!(
            Mapper {
                c8y: {
                    url: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfig {
                pub c8y: C8yConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct C8yConfig {
                pub url: OptionalConfig<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn field_with_from_key_via_default_is_optional() {
        let input: Configuration = parse_quote!(
            Mapper {
                device: {
                    #[tedge_config(default(from_key_via(
                        key = "device.cert_path",
                        function = "device_id_from_cert"
                    )))]
                    id: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfig {
                pub device: DeviceConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct DeviceConfig {
                pub id: OptionalConfig<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn nested_group_uses_parent_prefix() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    proxy: {
                        #[tedge_config(default(value = "8001"))]
                        port: u16,
                    },
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfig {
                pub c8y: C8yConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct C8yConfig {
                pub proxy: C8yProxyConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct C8yProxyConfig {
                pub port: u16,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn renamed_field_gets_facet_rename_only() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(rename = "type", default(value = "thin-edge.io"))]
                    ty: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfig {
                pub device: DeviceConfig,
            }

            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct DeviceConfig {
                #[facet(rename = "type")]
                pub ty: String,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn external_group_field_uses_the_schemas_reader_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                /// Device identity shared across mappers
                device: extern shared::MapperDeviceConfig,
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfig {
                /// Device identity shared across mappers
                pub device: shared::MapperDeviceConfig,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn root_struct_ident_spans_the_config_name() {
        let src = "Mapper {
    c8y: {
        url: String,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        assert_eq!(
            ident_starts(&generated, "MapperConfig"),
            vec![position_of(src, "Mapper")],
        );
    }

    #[test]
    fn group_struct_idents_span_the_group_name() {
        let src = "Mapper {
    c8y: {
        url: String,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        let starts = ident_starts(&generated, "C8yConfig");
        let expected = position_of(src, "c8y");
        // The ident appears both as the field type and the struct definition
        assert_eq!(starts.len(), 2);
        assert!(starts.iter().all(|start| *start == expected));
    }

    #[test]
    fn optional_config_wrapper_spans_the_field_type() {
        let src = "Mapper {
    c8y: {
        url: String,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        let expected = position_of(src, "String");
        assert_eq!(ident_starts(&generated, "OptionalConfig"), vec![expected]);
        assert_eq!(ident_starts(&generated, "String"), vec![expected]);
    }

    #[test]
    fn external_group_field_type_spans_the_extern_type() {
        let src = "Mapper {
    device: extern MapperDeviceConfig,
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        assert_eq!(
            ident_starts(&generated, "MapperDeviceConfig"),
            vec![position_of(src, "MapperDeviceConfig")],
        );
    }
}
