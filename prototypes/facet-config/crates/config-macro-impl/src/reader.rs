use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::dto::struct_name_for_group;
use crate::input::{ConfigField, ConfigGroup, Configuration, FieldDefault, FieldOrGroup};

pub fn generate_reader(config: &Configuration, root_name: &str) -> TokenStream {
    let root_ident = format_ident!("{root_name}Config");
    let mut structs = Vec::new();

    let fields = generate_reader_fields(&config.groups, "", &mut structs);

    structs.insert(
        0,
        quote! {
            #[derive(Debug, ::facet::Facet)]
            #[facet(type_tag = "config_group")]
            pub struct #root_ident {
                #(#fields)*
            }
        },
    );

    quote! { #(#structs)* }
}

fn generate_reader_fields(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    parent_prefix: &str,
    structs: &mut Vec<TokenStream>,
) -> Vec<TokenStream> {
    let mut fields = Vec::new();
    for item in items {
        match item {
            FieldOrGroup::Field(f) => fields.push(generate_reader_leaf(f)),
            FieldOrGroup::Group(g) => {
                let (field_token, nested) = generate_reader_group(g, parent_prefix);
                fields.push(field_token);
                structs.extend(nested);
            }
        }
    }
    fields
}

fn generate_reader_leaf(field: &ConfigField) -> TokenStream {
    let ident = field.ident.as_ref().unwrap();
    let ty = &field.ty;
    let doc_attrs = &field.attrs;

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
        quote! {
            #(#doc_attrs)*
            #(#extra_attrs)*
            pub #ident: OptionalConfig<#ty>,
        }
    }
}

fn generate_reader_group(
    group: &ConfigGroup,
    parent_prefix: &str,
) -> (TokenStream, Vec<TokenStream>) {
    let group_ident = &group.ident;
    let base_name = struct_name_for_group(parent_prefix, &group.ident.to_string());
    let struct_name = format!("{base_name}Config");
    let struct_ident = format_ident!("{struct_name}");
    let doc_attrs = &group.doc_attrs;

    let child_prefix = if parent_prefix.is_empty() {
        group.ident.to_string()
    } else {
        format!("{parent_prefix}_{}", group.ident)
    };

    let mut nested_structs = Vec::new();
    let fields = generate_reader_fields(&group.contents, &child_prefix, &mut nested_structs);

    let struct_def = quote! {
        #[derive(Debug, ::facet::Facet)]
        #[facet(type_tag = "config_group")]
        pub struct #struct_ident {
            #(#fields)*
        }
    };

    nested_structs.insert(0, struct_def);

    let field_token = quote! {
        #(#doc_attrs)*
        pub #group_ident: #struct_ident,
    };

    (field_token, nested_structs)
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
    fn field_with_default_is_concrete_type() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    #[tedge_config(default(value = "1883"))]
                    port: u16,
                },
            }
        );
        let generated = generate_reader(&input, &input.name.to_string());
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
        let generated = generate_reader(&input, &input.name.to_string());
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
        let generated = generate_reader(&input, &input.name.to_string());
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
        let generated = generate_reader(&input, &input.name.to_string());
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
}
