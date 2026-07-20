//! Parses the declaration passed to `define_config!`.
//!
//! Generated names and complete config keys are added later by the model.

use darling::FromAttributes;
use darling::FromMeta;
use syn::parse::Parse;
use syn::punctuated::Punctuated;
use syn::Attribute;
use syn::Token;

#[derive(Debug)]
pub struct Configuration {
    pub name: syn::Ident,
    pub groups: Punctuated<FieldOrGroup, Token![,]>,
}

#[derive(Debug)]
pub enum FieldOrGroup {
    Field(Box<ConfigField>),
    Group(ConfigGroup),
    ExternalGroup(Box<ConfigExternalGroup>),
}

#[derive(Debug)]
pub struct ConfigGroup {
    pub doc_attrs: Vec<syn::Attribute>,
    pub ident: syn::Ident,
    pub contents: Punctuated<FieldOrGroup, Token![,]>,
}

#[derive(Debug)]
pub struct ConfigField {
    pub doc_attrs: Vec<syn::Attribute>,
    pub readonly: bool,
    rename: Option<String>,
    pub deprecated_key: Option<String>,
    pub default: Option<FieldDefault>,
    pub examples: Vec<String>,
    ident: syn::Ident,
    pub ty: syn::Type,
}

#[derive(Debug)]
pub struct ConfigExternalGroup {
    pub doc_attrs: Vec<syn::Attribute>,
    pub ident: syn::Ident,
    pub ty: syn::Type,
}

#[derive(Debug, FromMeta)]
pub enum FieldDefault {
    Value(String),
    Function(syn::Path),
    FromKey(String),
    FromOptionalKey(String),
    FromConfigDir(String),
    FromRoot(String),
    FromKeyVia(FromKeyVia),
}

/// A default derived from another key's resolved value via a function
#[derive(Debug, FromMeta)]
pub struct FromKeyVia {
    pub key: String,
    pub function: syn::Path,
}

#[derive(FromAttributes, Debug)]
#[darling(attributes(tedge_config))]
struct FieldAttributes {
    #[darling(default)]
    readonly: bool,
    #[darling(default)]
    rename: Option<String>,
    #[darling(default)]
    deprecated_key: Option<String>,
    #[darling(default)]
    default: Option<FieldDefault>,
    #[darling(multiple, rename = "example")]
    examples: Vec<String>,
}

// Empty by design: `tedge_config` options only apply to fields.
#[derive(FromAttributes, Debug)]
#[darling(attributes(tedge_config))]
struct GroupAttributes {}

// Empty by design: options belong where the external schema is declared,
// not where it is reused.
#[derive(FromAttributes, Debug)]
#[darling(attributes(tedge_config))]
struct ExternalGroupAttributes {}

impl Parse for Configuration {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: syn::Ident = input.parse()?;
        let content;
        syn::braced!(content in input);
        let groups = content.parse_terminated(<_>::parse, Token![,])?;
        Ok(Self { name, groups })
    }
}

impl Parse for FieldOrGroup {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let ident: syn::Ident = input.parse()?;
        input.parse::<Token![:]>()?;

        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            parse_attributes::<GroupAttributes>(&attrs)?;
            let contents = content.parse_terminated(<_>::parse, Token![,])?;
            Ok(Self::Group(ConfigGroup {
                doc_attrs: doc_attrs(attrs),
                ident,
                contents,
            }))
        } else if input.peek(Token![extern]) {
            input.parse::<Token![extern]>()?;
            let ty: syn::Type = input.parse()?;
            parse_attributes::<ExternalGroupAttributes>(&attrs)?;
            Ok(Self::ExternalGroup(Box::new(ConfigExternalGroup {
                doc_attrs: doc_attrs(attrs),
                ident,
                ty,
            })))
        } else {
            let ty: syn::Type = input.parse()?;
            let options = parse_attributes::<FieldAttributes>(&attrs)?;
            Ok(Self::Field(Box::new(ConfigField {
                doc_attrs: doc_attrs(attrs),
                readonly: options.readonly,
                rename: options.rename,
                deprecated_key: options.deprecated_key,
                default: options.default,
                examples: options.examples,
                ident,
                ty,
            })))
        }
    }
}

fn parse_attributes<T: FromAttributes>(attrs: &[syn::Attribute]) -> syn::Result<T> {
    T::from_attributes(attrs).map_err(|e| syn::Error::new(e.span(), e.to_string()))
}

fn doc_attrs(attrs: Vec<syn::Attribute>) -> Vec<syn::Attribute> {
    attrs
        .into_iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .collect()
}

impl ConfigField {
    /// Returns the Rust identifier used for struct field names in generated code
    pub fn field_ident(&self) -> &syn::Ident {
        &self.ident
    }

    /// Returns the config key name with any rename applied
    pub fn config_name(&self) -> String {
        self.rename
            .clone()
            .unwrap_or_else(|| self.ident.to_string())
    }

    /// Returns the explicit rename, if one was declared
    pub fn rename(&self) -> Option<&str> {
        self.rename.as_deref()
    }
}

impl ConfigGroup {
    pub fn config_name(&self) -> String {
        self.ident.to_string()
    }
}

impl ConfigExternalGroup {
    pub fn config_name(&self) -> String {
        self.ident.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn parse_simple_group_with_fields() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    port: u16,
                    host: String,
                },
            }
        );
        assert_eq!(config.groups.len(), 1);
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                assert_eq!(g.ident, "mqtt");
                assert_eq!(g.contents.len(), 2);
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_nested_groups() {
        let config: Configuration = parse_quote!(
            Test {
                c8y: {
                    url: String,
                    proxy: {
                        port: u16,
                    },
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                assert_eq!(g.contents.len(), 2);
                match &g.contents[1] {
                    FieldOrGroup::Group(nested) => assert_eq!(nested.ident, "proxy"),
                    _ => panic!("expected nested group"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_field_attributes() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    #[tedge_config(default(value = "1883"), readonly)]
                    port: u16,

                    #[tedge_config(rename = "type")]
                    ty: String,

                    #[tedge_config(deprecated_key = "mqtt.external.host")]
                    host: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert!(f.readonly);
                        assert!(matches!(&f.default, Some(FieldDefault::Value(v)) if v == "1883"));
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.rename.as_deref(), Some("type"));
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[2] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.deprecated_key.as_deref(), Some("mqtt.external.host"));
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_external_group() {
        let config: Configuration = parse_quote!(
            Mapper {
                device: extern MapperDeviceConfig,
            }
        );
        match &config.groups[0] {
            FieldOrGroup::ExternalGroup(g) => {
                assert_eq!(g.ident, "device");
                let expected: syn::Type = parse_quote!(MapperDeviceConfig);
                assert_eq!(g.ty, expected);
            }
            other => panic!("expected external group, got {other:?}"),
        }
    }

    #[test]
    fn parse_external_group_with_path_type() {
        let config: Configuration = parse_quote!(
            Mapper {
                device: extern crate::shared::MapperDeviceConfig,
            }
        );
        match &config.groups[0] {
            FieldOrGroup::ExternalGroup(g) => {
                let expected: syn::Type = parse_quote!(crate::shared::MapperDeviceConfig);
                assert_eq!(g.ty, expected);
            }
            other => panic!("expected external group, got {other:?}"),
        }
    }

    #[test]
    fn parse_external_group_nested_in_group() {
        let config: Configuration = parse_quote!(
            Mapper {
                c8y: {
                    device: extern MapperDeviceConfig,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                assert!(matches!(&g.contents[0], FieldOrGroup::ExternalGroup(_)));
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_external_group_doc_comments_preserved() {
        let config: Configuration = parse_quote!(
            Mapper {
                /// Device identity shared across mappers
                device: extern MapperDeviceConfig,
            }
        );
        match &config.groups[0] {
            FieldOrGroup::ExternalGroup(g) => {
                assert_eq!(g.doc_attrs.len(), 1);
                assert!(g.doc_attrs[0].path().is_ident("doc"));
            }
            other => panic!("expected external group, got {other:?}"),
        }
    }

    #[test]
    fn external_group_attributes_are_rejected() {
        let err = syn::parse_str::<Configuration>(
            "Mapper {
                #[tedge_config(default(value = \"1883\"))]
                device: extern MapperDeviceConfig,
            }",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Unknown field: `default`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn external_group_followed_by_braces_is_an_error() {
        let err = syn::parse_str::<Configuration>(
            "Mapper {
                device: extern { id: String },
            }",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("expected"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn group_attributes_are_rejected() {
        let err = syn::parse_str::<Configuration>(
            "Test {
                #[tedge_config(multi)]
                c8y: {
                    url: String,
                },
            }",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Unknown field: `multi`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn parse_doc_comments_preserved() {
        let config: Configuration = parse_quote!(
            Test {
                mqtt: {
                    /// MQTT broker port
                    port: u16,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => match &g.contents[0] {
                FieldOrGroup::Field(f) => {
                    assert_eq!(f.doc_attrs.len(), 1);
                    assert!(f.doc_attrs[0].path().is_ident("doc"));
                }
                _ => panic!("expected field"),
            },
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_default_variants() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(default(from_key = "other.field"))]
                    cert_path: String,

                    #[tedge_config(default(from_config_dir = "certs/cert.pem"))]
                    key_path: String,

                    #[tedge_config(default(from_optional_key = "c8y.url"))]
                    url: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromKey(k)) if k == "other.field")
                        );
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromConfigDir(p)) if p == "certs/cert.pem")
                        );
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[2] {
                    FieldOrGroup::Field(f) => {
                        assert!(
                            matches!(&f.default, Some(FieldDefault::FromOptionalKey(k)) if k == "c8y.url")
                        );
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_from_key_via_default() {
        let config: Configuration = parse_quote!(
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
        match &config.groups[0] {
            FieldOrGroup::Group(g) => match &g.contents[0] {
                FieldOrGroup::Field(f) => match &f.default {
                    Some(FieldDefault::FromKeyVia(via)) => {
                        assert_eq!(via.key, "device.cert_path");
                        assert!(via.function.is_ident("device_id_from_cert"));
                    }
                    other => panic!("expected from_key_via default, got {other:?}"),
                },
                _ => panic!("expected field"),
            },
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn parse_example_attributes() {
        let config: Configuration = parse_quote!(
            Test {
                device: {
                    #[tedge_config(example = "my-device", example = "AINA123")]
                    id: String,

                    name: String,
                },
            }
        );
        match &config.groups[0] {
            FieldOrGroup::Group(g) => {
                match &g.contents[0] {
                    FieldOrGroup::Field(f) => {
                        assert_eq!(f.examples, vec!["my-device", "AINA123"]);
                    }
                    _ => panic!("expected field"),
                }
                match &g.contents[1] {
                    FieldOrGroup::Field(f) => {
                        assert!(f.examples.is_empty());
                    }
                    _ => panic!("expected field"),
                }
            }
            _ => panic!("expected group"),
        }
    }
}
