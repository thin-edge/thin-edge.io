use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::input::{ConfigField, ConfigGroup, Configuration, FieldOrGroup};

pub fn generate_dto(config: &Configuration, root_name: &str) -> TokenStream {
    let root_ident = format_ident!("{root_name}ConfigDto");
    let mut structs = Vec::new();

    let fields = generate_group_fields(&config.groups, "", &mut structs);

    structs.insert(
        0,
        quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct #root_ident {
                #(#fields)*
            }
        },
    );

    quote! { #(#structs)* }
}

pub(crate) fn struct_name_for_group(parent_prefix: &str, group_name: &str) -> String {
    if parent_prefix.is_empty() {
        group_name.to_upper_camel_case()
    } else {
        format!(
            "{}{}",
            parent_prefix.to_upper_camel_case(),
            group_name.to_upper_camel_case()
        )
    }
}

fn generate_group_fields(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    parent_prefix: &str,
    structs: &mut Vec<TokenStream>,
) -> Vec<TokenStream> {
    let mut fields = Vec::new();
    for item in items {
        match item {
            FieldOrGroup::Field(f) => fields.push(generate_leaf_field(f)),
            FieldOrGroup::Group(g) => {
                let (field_token, nested_structs) = generate_group(g, parent_prefix);
                fields.push(field_token);
                structs.extend(nested_structs);
            }
        }
    }
    fields
}

fn generate_leaf_field(field: &ConfigField) -> TokenStream {
    let ident = field.ident.as_ref().unwrap();
    let ty = &field.ty;
    let doc_attrs = &field.attrs;

    let mut extra_attrs = Vec::new();
    if let Some(rename) = &field.rename {
        extra_attrs.push(quote! { #[facet(rename = #rename)] });
        extra_attrs.push(quote! { #[serde(rename = #rename)] });
    }

    quote! {
        #(#doc_attrs)*
        #(#extra_attrs)*
        pub #ident: Option<#ty>,
    }
}

fn generate_group(group: &ConfigGroup, parent_prefix: &str) -> (TokenStream, Vec<TokenStream>) {
    let group_ident = &group.ident;
    let base_name = struct_name_for_group(parent_prefix, &group.ident.to_string());
    let struct_name = format!("{base_name}ConfigDto");
    let struct_ident = format_ident!("{struct_name}");
    let doc_attrs = &group.doc_attrs;

    let child_prefix = if parent_prefix.is_empty() {
        group.ident.to_string()
    } else {
        format!("{parent_prefix}_{}", group.ident)
    };

    let mut nested_structs = Vec::new();
    let fields = generate_group_fields(&group.contents, &child_prefix, &mut nested_structs);

    let mut all_fields: Vec<TokenStream> = fields;

    if group.multi {
        let profile_name = format!("{base_name}ProfileDto");
        let profile_ident = format_ident!("{profile_name}");

        all_fields.push(quote! {
            #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
            pub profiles: std::collections::HashMap<String, #profile_ident>,
        });

        let profile_fields = generate_group_fields_for_profile(&group.contents, &child_prefix);
        nested_structs.push(quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct #profile_ident {
                #(#profile_fields)*
            }
        });
    }

    let struct_def = quote! {
        #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
        #[facet(type_tag = "config_group")]
        pub struct #struct_ident {
            #(#all_fields)*
        }
    };

    nested_structs.insert(0, struct_def);

    let field_token = quote! {
        #(#doc_attrs)*
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub #group_ident: Option<#struct_ident>,
    };

    (field_token, nested_structs)
}

fn generate_group_fields_for_profile(
    items: &syn::punctuated::Punctuated<FieldOrGroup, syn::Token![,]>,
    parent_prefix: &str,
) -> Vec<TokenStream> {
    let mut fields = Vec::new();
    for item in items {
        match item {
            FieldOrGroup::Field(f) => fields.push(generate_leaf_field(f)),
            FieldOrGroup::Group(g) => {
                let group_ident = &g.ident;
                let base_name = struct_name_for_group(parent_prefix, &g.ident.to_string());
                let struct_name = format!("{base_name}ConfigDto");
                let struct_ident = format_ident!("{struct_name}");
                let doc_attrs = &g.doc_attrs;
                fields.push(quote! {
                    #(#doc_attrs)*
                    #[serde(default, skip_serializing_if = "Option::is_none")]
                    pub #group_ident: Option<#struct_ident>,
                });
            }
        }
    }
    fields
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
    fn simple_group_with_leaf_fields() {
        let input: Configuration = parse_quote!(
            Mapper {
                mqtt: {
                    port: u16,
                    host: String,
                },
            }
        );
        let generated = generate_dto(&input, &input.name.to_string());
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
        let generated = generate_dto(&input, &input.name.to_string());
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
        let generated = generate_dto(&input, &input.name.to_string());
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
        let generated = generate_dto(&input, &input.name.to_string());
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
        let generated = generate_dto(&input, &input.name.to_string());
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
    fn multi_group_generates_profiles_field_and_profile_struct() {
        let input: Configuration = parse_quote!(
            Test {
                #[tedge_config(multi)]
                c8y: {
                    url: String,
                    device: {
                        cert_path: String,
                    },
                },
            }
        );
        let generated = generate_dto(&input, &input.name.to_string());
        let expected: TokenStream = parse_quote! {
            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct TestConfigDto {
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub c8y: Option<C8yConfigDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yConfigDto {
                pub url: Option<String>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<C8yDeviceConfigDto>,
                #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
                pub profiles: std::collections::HashMap<String, C8yProfileDto>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yDeviceConfigDto {
                pub cert_path: Option<String>,
            }

            #[derive(Debug, Default, Clone, ::facet::Facet, ::serde::Serialize, ::serde::Deserialize)]
            #[facet(type_tag = "config_group")]
            pub struct C8yProfileDto {
                pub url: Option<String>,
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub device: Option<C8yDeviceConfigDto>,
            }
        };
        assert_eq(&generated, &expected);
    }
}
