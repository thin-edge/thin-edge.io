//! Generates the form used to deserialize and edit stored configuration.
//!
//! Its fields are optional so stored values remain distinct from defaults.
//! Read-only markers, deprecated key aliases, and example values are emitted
//! as facet attributes (`tedge::readonly`, `tedge::deprecated_key`,
//! `tedge::example`) so the runtime discovers them via shape-tree walks
//! instead of explicit codegen tables.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

use crate::input::ConfigField;
use crate::model::{GroupModel, ItemModel, Model};

pub fn generate_dto(model: &Model) -> TokenStream {
    let structs = generate_group(&model.root);
    quote! { #(#structs)* }
}

fn generate_group(group: &GroupModel) -> Vec<TokenStream> {
    let mut nested = Vec::new();
    let mut fields = Vec::new();

    for item in &group.items {
        match item {
            ItemModel::Field(f) => fields.push(generate_leaf_field(f.field)),
            ItemModel::Group(child) => {
                let ident = child.ident;
                let ty = &child.group.dto_ident;
                let doc_attrs = child.doc_attrs;
                fields.push(quote! {
                    #(#doc_attrs)*
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub #ident: Option<#ty>,
                });
                nested.extend(generate_group(&child.group));
            }
            ItemModel::External(ext) => {
                let ident = &ext.ext.ident;
                let ty = &ext.ext.ty;
                let doc_attrs = &ext.ext.doc_attrs;
                // Report an invalid external type at the caller's declaration.
                let field_ty = quote_spanned! {ty.span()=> Option<<#ty as ConfigSchema>::Dto> };
                fields.push(quote! {
                    #(#doc_attrs)*
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub #ident: #field_ty,
                });
            }
        }
    }

    let struct_ident = &group.dto_ident;
    let mut structs = vec![quote! {
        #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
        #[facet(type_tag = "config_group")]
        pub struct #struct_ident {
            #(#fields)*
        }
    }];
    structs.extend(nested);
    structs
}

fn generate_leaf_field(field: &ConfigField) -> TokenStream {
    let ident = &field.ident;
    let ty = &field.ty;
    let doc_attrs = &field.doc_attrs;

    let mut extra_attrs = Vec::new();
    if let Some(rename) = &field.rename {
        extra_attrs.push(quote! { #[facet(rename = #rename)] });
        extra_attrs.push(quote! { #[serde(rename = #rename)] });
    }
    if field.readonly {
        extra_attrs.push(quote! { #[facet(tedge::readonly)] });
    }
    if let Some(deprecated_key) = &field.deprecated_key {
        extra_attrs.push(quote! { #[facet(tedge::deprecated_key = #deprecated_key)] });
    }
    for example in &field.examples {
        extra_attrs.push(quote! { #[facet(tedge::example = #example)] });
    }

    let field_ty = quote_spanned! {ty.span()=> Option<#ty> };

    quote! {
        #(#doc_attrs)*
        #(#extra_attrs)*
        pub #ident: #field_ty,
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
        generate_dto(&Model::new(config))
    }

    #[test]
    fn simple_group_with_leaf_fields() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    port: u16,
                    host: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub mqtt: Option<MqttConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MqttConfigDto {
                pub port: Option<u16>,
                pub host: Option<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn nested_groups_use_parent_prefix() {
        let input: Configuration = parse_quote!(
            Mapper {
                c8y: {
                    url: String,
                    proxy: {
                        port: u16,
                    },
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub c8y: Option<C8yConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yConfigDto {
                pub url: Option<String>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub proxy: Option<C8yProxyConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yProxyConfigDto {
                pub port: Option<u16>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn same_group_name_at_different_levels_no_conflict() {
        let input: Configuration = parse_quote!(
            Test {
                c8y: {
                    device: {
                        cert_path: String,
                    },
                },
                device: {
                    id: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub c8y: Option<C8yConfigDto>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<DeviceConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<C8yDeviceConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yDeviceConfigDto {
                pub cert_path: Option<String>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct DeviceConfigDto {
                pub id: Option<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn doc_comments_preserved() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    /// MQTT broker port
                    port: u16,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub mqtt: Option<MqttConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MqttConfigDto {
                /// MQTT broker port
                pub port: Option<u16>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn renamed_field_gets_facet_and_serde_attrs() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(rename = "type")]
                    ty: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<DeviceConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct DeviceConfigDto {
                #[facet(rename = "type")]
                #[serde(rename = "type")]
                pub ty: Option<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn external_group_field_projects_the_schemas_dto_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                /// Device identity shared across mappers
                device: extern shared::MapperDeviceConfig,
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MapperConfigDto {
                /// Device identity shared across mappers
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<<shared::MapperDeviceConfig as ConfigSchema>::Dto>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn readonly_field_gets_facet_attr() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(readonly)]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub mqtt: Option<MqttConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MqttConfigDto {
                #[facet(tedge::readonly)]
                pub port: Option<u16>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn deprecated_key_field_gets_facet_attr() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(deprecated_key = "mqtt.external.port")]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub mqtt: Option<MqttConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct MqttConfigDto {
                #[facet(tedge::deprecated_key = "mqtt.external.port")]
                pub port: Option<u16>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn example_fields_get_facet_attrs() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(example = "my-device", example = "AINA123")]
                    id: String,
                },
            }
        );
        let generated = generate(&input);
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<DeviceConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct DeviceConfigDto {
                #[facet(tedge::example = "my-device")]
                #[facet(tedge::example = "AINA123")]
                pub id: Option<String>,
            }
        };
        assert_eq(&generated, &expected);
    }

    #[test]
    fn root_struct_ident_spans_the_config_name() {
        let src = "Mapper {
    mqtt: {
        port: u16,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        assert_eq!(
            ident_starts(&generated, "MapperConfigDto"),
            vec![position_of(src, "Mapper")],
        );
    }

    #[test]
    fn group_struct_idents_span_the_group_name() {
        let src = "Mapper {
    mqtt: {
        port: u16,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        let starts = ident_starts(&generated, "MqttConfigDto");
        let expected = position_of(src, "mqtt");
        // The ident appears both as the field type and the struct definition
        assert_eq!(starts.len(), 2);
        assert!(starts.iter().all(|start| *start == expected));
    }

    #[test]
    fn option_wrapper_spans_the_field_type() {
        let src = "Mapper {
    mqtt: {
        port: u16,
    },
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        let expected = position_of(src, "u16");
        assert!(ident_starts(&generated, "Option").contains(&expected));
        assert_eq!(ident_starts(&generated, "u16"), vec![expected]);
    }

    #[test]
    fn external_group_projection_spans_the_extern_type() {
        let src = "Mapper {
    device: extern MapperDeviceConfig,
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        let expected = position_of(src, "MapperDeviceConfig");
        assert_eq!(
            ident_starts(&generated, "MapperDeviceConfig"),
            vec![expected],
        );
        assert_eq!(ident_starts(&generated, "ConfigSchema"), vec![expected]);
    }
}
