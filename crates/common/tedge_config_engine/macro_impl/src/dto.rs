//! Generates the DTO (Data Transfer Object) structs used to deserialize and
//! persist stored configuration.
//!
//! Every leaf field is wrapped in `Option<T>` so an explicitly set value is
//! distinguishable from "not present" — defaults are never stored, only applied
//! at read time by the Reader (see [`super::reader`]).
//! Read-only markers, deprecated key aliases, and example values are emitted
//! as facet attributes (`tedge::readonly`, `tedge::deprecated_key`,
//! `tedge::example`) so the runtime discovers them via shape-tree walks.

use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::spanned::Spanned;

use crate::input::ConfigField;
use crate::model::GroupModel;
use crate::model::ItemModel;
use crate::model::Model;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GroupKind {
    SchemaRoot,
    Nested,
}

pub fn generate_dto(model: &Model) -> TokenStream {
    let structs = generate_group(&model.root, GroupKind::SchemaRoot);
    quote! { #(#structs)* }
}

fn generate_group(group: &GroupModel, kind: GroupKind) -> Vec<TokenStream> {
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
                nested.extend(generate_group(&child.group, GroupKind::Nested));
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

    let schema_root_attr =
        (kind == GroupKind::SchemaRoot).then(|| quote! { #[facet(tedge::schema_root)] });

    let struct_ident = &group.dto_ident;
    let mut structs = vec![quote! {
        #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
        #[facet(type_tag = "config_group")]
        #schema_root_attr
        pub struct #struct_ident {
            #(#fields)*
        }
    }];
    structs.extend(nested);
    structs
}

fn generate_leaf_field(field: &ConfigField) -> TokenStream {
    let ident = field.field_ident();
    let ty = &field.ty;
    let doc_attrs = &field.doc_attrs;

    let mut extra_attrs = Vec::new();
    if let Some(rename) = field.rename() {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub #ident: #field_ty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Configuration;
    use crate::test_utils::ident_positions;
    use crate::test_utils::position_of;
    use crate::test_utils::TokenQuery;
    use syn::parse_quote;

    #[test]
    fn leaf_fields_are_optional_and_skip_serializing_when_none() {
        let input: Configuration = parse_quote!(Mapper { port: u16 });
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MapperConfigDto")
            .find_field("port")
            .assert_eq(&parse_quote! {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub port: Option<u16>,
            });
    }

    #[test]
    fn root_dto_is_marked_as_the_schema_root() {
        let input: Configuration = parse_quote!(Test {});
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("TestConfigDto")
            .assert_eq(&parse_quote! {
                #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
                #[facet(type_tag = "config_group")]
                #[facet(tedge::schema_root)]
                pub struct TestConfigDto {}
            });
    }

    #[test]
    fn nested_group_dto_types_include_the_parent_group_name() {
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

        // `proxy` is nested under `c8y`, so its type includes both group names.
        TokenQuery::new(&generated)
            .find_struct("C8yConfigDto")
            .find_field("proxy")
            .assert_eq(&parse_quote! {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub proxy: Option<C8yProxyConfigDto>,
            });
    }

    #[test]
    fn same_named_groups_at_different_levels_get_distinct_dto_types() {
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

        TokenQuery::new(&generated)
            .find_struct("TestConfigDto")
            .find_field("device")
            .assert_eq(&parse_quote! {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<DeviceConfigDto>,
            });
        TokenQuery::new(&generated)
            .find_struct("C8yConfigDto")
            .find_field("device")
            .assert_eq(&parse_quote! {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<C8yDeviceConfigDto>,
            });
    }

    #[test]
    fn field_doc_comments_are_preserved_in_the_dto() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    /// MQTT broker port
                    port: u16,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MqttConfigDto")
            .find_field("port")
            .assert_eq(&parse_quote! {
                /// MQTT broker port
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub port: Option<u16>,
            });
    }

    #[test]
    fn renamed_fields_get_facet_and_serde_attributes() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(rename = "type")]
                    ty: String,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("DeviceConfigDto")
            .find_field("ty")
            .assert_eq(&parse_quote! {
                #[facet(rename = "type")]
                #[serde(rename = "type")]
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub ty: Option<String>,
            });
    }

    #[test]
    fn external_group_fields_use_the_external_schemas_dto_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                /// Device identity shared across mappers
                device: extern shared::MapperDeviceConfig,
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MapperConfigDto")
            .find_field("device")
            .assert_eq(&parse_quote! {
                /// Device identity shared across mappers
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<<shared::MapperDeviceConfig as ConfigSchema>::Dto>,
            });
    }

    #[test]
    fn readonly_fields_get_the_facet_attribute() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(readonly)]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MqttConfigDto")
            .find_field("port")
            .assert_eq(&parse_quote! {
                #[facet(tedge::readonly)]
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub port: Option<u16>,
            });
    }

    #[test]
    fn fields_with_deprecated_keys_get_the_facet_attribute() {
        let input: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(deprecated_key = "mqtt.external.port")]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MqttConfigDto")
            .find_field("port")
            .assert_eq(&parse_quote! {
                #[facet(tedge::deprecated_key = "mqtt.external.port")]
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub port: Option<u16>,
            });
    }

    #[test]
    fn fields_with_examples_get_facet_attributes() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(example = "my-device", example = "AINA123")]
                    id: String,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("DeviceConfigDto")
            .find_field("id")
            .assert_eq(&parse_quote! {
                #[facet(tedge::example = "my-device")]
                #[facet(tedge::example = "AINA123")]
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub id: Option<String>,
            });
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
            ident_positions(&generated, "MapperConfigDto"),
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
        let starts = ident_positions(&generated, "MqttConfigDto");
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
        assert!(ident_positions(&generated, "Option").contains(&expected));
        assert_eq!(ident_positions(&generated, "u16"), vec![expected]);
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
            ident_positions(&generated, "MapperDeviceConfig"),
            vec![expected],
        );
        assert_eq!(ident_positions(&generated, "ConfigSchema"), vec![expected]);
    }

    fn generate(config: &Configuration) -> TokenStream {
        generate_dto(&Model::new(config))
    }
}
