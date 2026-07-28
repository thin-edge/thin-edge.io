//! Generates the typed configuration returned to application code.
//!
//! Fields with guaranteed defaults are plain values. Other fields retain
//! their key so an unset value can produce a useful error when accessed.
//! Each reader group gets a `build_from_dto` method that constructs itself
//! directly from the DTO + defaults, without requiring Facet on Reader types.

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
    let mut build_fields = Vec::new();

    for item in &group.items {
        match item {
            ItemModel::Field(f) => {
                fields.push(generate_reader_leaf(f.field));
                build_fields.push(generate_build_field(f.field, &f.key));
            }
            ItemModel::Group(child) => {
                let ident = child.ident;
                let ty = &child.group.reader_ident;
                let doc_attrs = child.doc_attrs;
                fields.push(quote! {
                    #(#doc_attrs)*
                    pub #ident: #ty,
                });
                build_fields.push(quote! {
                    #ident: <#ty as tedge::BuildFromDto>::build_from_dto(__dto, __defaults, __root, __display_prefix, __profile)?,
                });
                nested.extend(generate_group(&child.group));
            }
            ItemModel::External(ext) => {
                let ident = &ext.ext.ident;
                let ty = &ext.ext.ty;
                let doc_attrs = &ext.ext.doc_attrs;
                let field_ty = quote_spanned! {ty.span()=> #ty };
                fields.push(quote! {
                    #(#doc_attrs)*
                    pub #ident: #field_ty,
                });
                let _prefix = &ext.prefix;
                build_fields.push(quote! {
                    #ident: <#ty as tedge::BuildFromDto>::build_from_dto(
                        __dto,
                        __defaults,
                        __root,
                        __display_prefix,
                        __profile,
                    )?,
                });
            }
        }
    }

    let struct_ident = &group.reader_ident;
    let mut structs = vec![quote! {
        #[derive(Debug)]
        pub struct #struct_ident {
            #(#fields)*
        }

        impl tedge::BuildFromDto for #struct_ident {
            fn build_from_dto<__Dto: for<'a> ::facet::Facet<'a>>(
                __dto: &__Dto,
                __defaults: &tedge::DefaultsRegistry,
                __root: tedge::RootResolver<'_>,
                __display_prefix: &str,
                __profile: Option<&str>,
            ) -> Result<Self, tedge::ConfigError> {
                Ok(Self {
                    #(#build_fields)*
                })
            }
        }
    }];
    structs.extend(nested);
    structs
}

fn has_concrete_default(field: &ConfigField) -> bool {
    matches!(
        &field.default,
        Some(
            FieldDefault::Value(_)
                | FieldDefault::Function(_)
                | FieldDefault::FromConfigDir(_)
                | FieldDefault::FromKey(_)
        )
    )
}

fn generate_reader_leaf(field: &ConfigField) -> TokenStream {
    let ident = field.field_ident();
    let ty = &field.ty;
    let doc_attrs = &field.doc_attrs;

    if has_concrete_default(field) {
        quote! {
            #(#doc_attrs)*
            pub #ident: #ty,
        }
    } else {
        let field_ty = quote_spanned! {ty.span()=> OptionalConfig<#ty> };
        quote! {
            #(#doc_attrs)*
            pub #ident: #field_ty,
        }
    }
}

fn generate_build_field(field: &ConfigField, key: &str) -> TokenStream {
    let ident = field.field_ident();

    if has_concrete_default(field) {
        quote! {
            #ident: tedge::reader_helpers::read_required(__dto, __defaults, __root, #key)?,
        }
    } else {
        quote! {
            #ident: tedge::reader_helpers::read_optional(__dto, __defaults, __root, #key, __display_prefix, __profile)?,
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
    fn renamed_reader_fields_do_not_get_facet_attributes() {
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
        let expected = position_of(src, "Mapper");
        let positions = ident_positions(&generated, "MapperConfig");
        assert!(positions.len() >= 1);
        assert!(positions.iter().all(|p| *p == expected));
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
        // The ident appears as field type, struct def, and in the impl block
        assert!(starts.len() >= 2);
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
        let expected = position_of(src, "MapperDeviceConfig");
        let positions = ident_positions(&generated, "MapperDeviceConfig");
        assert!(positions.len() >= 1);
        assert!(positions.iter().all(|p| *p == expected));
    }

    fn generate(config: &Configuration) -> TokenStream {
        generate_reader(&Model::new(config))
    }
}
