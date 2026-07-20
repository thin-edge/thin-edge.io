//! Generates the typed configuration returned to application code.
//!
//! Fields with guaranteed defaults are plain values. Other fields retain
//! their key so an unset value can produce a useful error when accessed.

use proc_macro2::TokenStream;
use quote::quote;
use quote::quote_spanned;
use syn::spanned::Spanned;

use crate::input::ConfigField;
use crate::input::FieldDefault;
use crate::model::GroupModel;
use crate::model::ItemModel;
use crate::model::Model;

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
    let ident = field.field_ident();
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
    if let Some(rename) = field.rename() {
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
    use crate::test_utils::ident_positions;
    use crate::test_utils::position_of;
    use crate::test_utils::TokenQuery;
    use syn::parse_quote;

    #[test]
    fn fields_with_defaults_have_concrete_reader_types() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    #[tedge_config(default(value = "1883"))]
                    port: u16,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MqttConfig")
            .find_field("port")
            .assert_eq(&parse_quote!(pub port: u16,));
    }

    #[test]
    fn fields_without_defaults_have_optional_reader_types() {
        let input: Configuration = parse_quote!(
            Mapper {
                c8y: {
                    url: String,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("C8yConfig")
            .find_field("url")
            .assert_eq(&parse_quote!(pub url: OptionalConfig<String>,));
    }

    #[test]
    fn fields_with_fallible_derived_defaults_have_optional_reader_types() {
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

        TokenQuery::new(&generated)
            .find_struct("DeviceConfig")
            .find_field("id")
            .assert_eq(&parse_quote!(pub id: OptionalConfig<String>,));
    }

    #[test]
    fn nested_group_reader_types_include_the_parent_group_name() {
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

        TokenQuery::new(&generated)
            .find_struct("C8yConfig")
            .find_field("proxy")
            .assert_eq(&parse_quote!(pub proxy: C8yProxyConfig,));
    }

    #[test]
    fn renamed_reader_fields_only_get_the_facet_attribute() {
        let input: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(rename = "type", default(value = "thin-edge.io"))]
                    ty: String,
                },
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("DeviceConfig")
            .find_field("ty")
            .assert_eq(&parse_quote! {
                #[facet(rename = "type")]
                pub ty: String,
            });
    }

    #[test]
    fn external_group_fields_use_the_external_schemas_reader_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                /// Device identity shared across mappers
                device: extern shared::MapperDeviceConfig,
            }
        );
        let generated = generate(&input);

        TokenQuery::new(&generated)
            .find_struct("MapperConfig")
            .find_field("device")
            .assert_eq(&parse_quote! {
                /// Device identity shared across mappers
                pub device: shared::MapperDeviceConfig,
            });
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
            ident_positions(&generated, "MapperConfig"),
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
        let starts = ident_positions(&generated, "C8yConfig");
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
        assert_eq!(
            ident_positions(&generated, "OptionalConfig"),
            vec![expected]
        );
        assert_eq!(ident_positions(&generated, "String"), vec![expected]);
    }

    #[test]
    fn external_group_field_type_spans_the_extern_type() {
        let src = "Mapper {
    device: extern MapperDeviceConfig,
}";
        let input: Configuration = syn::parse_str(src).unwrap();
        let generated = generate(&input);
        assert_eq!(
            ident_positions(&generated, "MapperDeviceConfig"),
            vec![position_of(src, "MapperDeviceConfig")],
        );
    }

    fn generate(config: &Configuration) -> TokenStream {
        generate_reader(&Model::new(config))
    }
}
